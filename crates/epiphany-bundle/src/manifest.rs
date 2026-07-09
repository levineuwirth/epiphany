//! The manifest: the table of roots and declarations (Chapter 8 §"The
//! Manifest").
//!
//! The manifest carries no user-facing data — only the structural roots
//! (operation-envelope blocks, the canonical base, blobs) and declarations
//! (profiles, extensions) the format needs to locate and validate content. It
//! is itself a content-addressed chunk, but a special one: it is the bootstrap
//! entry from the superblock into the chunk graph, so it is **mandatory
//! uncompressed** in this format version (Chapter 8 §"Manifest Encoding").
//!
//! Its fields partition cleanly into *canonical roots* that define the document
//! ([`Manifest::canonical_base`], [`Manifest::operation_roots`], canonical
//! [`Manifest::blob_roots`], the structural ids/declarations) and *non-canonical
//! accelerators* ([`Manifest::operation_index_root`],
//! [`Manifest::acceleration_snapshots`], the text-projection and integrity
//! roots), which may be rebuilt or discarded without altering canonical state
//! (Chapter 8 §"Canonical and Non-Canonical Manifest Roots"). The two snapshot
//! fields are deliberately distinct (QUICKSTART: *"these are distinct; do not
//! merge them"*): exactly one canonical base, plus any number of caches.

use std::collections::BTreeMap;

use crate::chunk::{ChunkRef, CompressionAlgorithm};
use crate::codec::{DecodeError, Reader, Writer};
use crate::ids::{
    BlobId, DocumentId, ExtensionId, FrontierBytes, LineageId, ManifestId,
    ReductionAlgorithmVersion, SchemaVersion, SemVer, WallClockDuration,
};
use crate::superblock::ProfileId;
use epiphany_determinism::{ChunkId, ContentHash};

/// A reference to a materialized snapshot (Chapter 8 §"Snapshots and Canonical
/// Bases"). The `covers_causal_frontier` is opaque to the bundle (a DVV the
/// semantic layer interprets).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SnapshotRef {
    /// Snapshot identity.
    pub snapshot_id: crate::ids::SnapshotId,
    /// The causal frontier the snapshot materializes (opaque DVV bytes).
    pub covers_causal_frontier: FrontierBytes,
    /// Reduction-algorithm version under which the snapshot was produced.
    pub reduction_algorithm_version: ReductionAlgorithmVersion,
    /// Profile under which the snapshot was produced.
    pub profile_id: ProfileId,
    /// Root chunk of the snapshot's materialized state.
    pub root: ChunkRef,
    /// Hash of the snapshot's root chunk, for fast verification.
    pub hash: ContentHash,
}

impl SnapshotRef {
    fn encode(&self, w: &mut Writer) {
        self.snapshot_id.encode(w);
        self.covers_causal_frontier.encode(w);
        self.reduction_algorithm_version.encode(w);
        self.profile_id.encode(w);
        self.root.encode(w);
        w.put_bytes(self.hash.as_bytes());
    }

    fn decode(r: &mut Reader) -> Result<Self, DecodeError> {
        Ok(SnapshotRef {
            snapshot_id: crate::ids::SnapshotId::decode(r)?,
            covers_causal_frontier: FrontierBytes::decode(r)?,
            reduction_algorithm_version: ReductionAlgorithmVersion::decode(r)?,
            profile_id: ProfileId::decode(r)?,
            root: ChunkRef::decode(r)?,
            hash: ContentHash(r.take_array::<32>()?),
        })
    }
}

/// A reference to a blob: large opaque content (Chapter 8 §"Blobs").
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct BlobRef {
    /// Blob identity (content hash under the `MUSCBLOB` domain).
    pub blob_id: BlobId,
    /// RFC 6838 media type.
    pub media_type: String,
    /// Offset of the on-disk payload.
    pub offset: u64,
    /// On-disk (compressed) length.
    pub compressed_length: u64,
    /// Uncompressed length.
    pub uncompressed_length: u64,
    /// Compression algorithm (metadata, not identity).
    pub compression: CompressionAlgorithm,
    /// Content hash, for verification.
    pub hash: ContentHash,
    /// Optional declared maximum size a reader may use to reject oversize blobs.
    pub declared_max_uncompressed_length: Option<u64>,
}

impl BlobRef {
    fn encode(&self, w: &mut Writer) {
        self.blob_id.encode(w);
        w.put_var_bytes(self.media_type.as_bytes());
        w.put_u64(self.offset);
        w.put_u64(self.compressed_length);
        w.put_u64(self.uncompressed_length);
        self.compression.encode(w);
        w.put_bytes(self.hash.as_bytes());
        w.put_opt(&self.declared_max_uncompressed_length, |w, v| {
            w.put_u64(*v);
        });
    }

    fn decode(r: &mut Reader) -> Result<Self, DecodeError> {
        let blob_id = BlobId::decode(r)?;
        let media_type = r.get_string()?;
        if !valid_media_type(&media_type) {
            return Err(DecodeError::Malformed(
                "blob media type is not a valid RFC 6838 type/subtype",
            ));
        }
        Ok(BlobRef {
            blob_id,
            media_type,
            offset: r.get_u64()?,
            compressed_length: r.get_u64()?,
            uncompressed_length: r.get_u64()?,
            compression: CompressionAlgorithm::decode(r)?,
            hash: ContentHash(r.take_array::<32>()?),
            declared_max_uncompressed_length: r.get_opt(|r| r.get_u64())?,
        })
    }
}

/// Whether `s` is a well-formed RFC 6838 `type/subtype` media type: ASCII (so
/// already Unicode-NFC, satisfying Appendix D §"Text and Unicode"), exactly one
/// `/`, and each side a valid *restricted name* per RFC 6838 §4.2 — 1..=127
/// characters, beginning alphanumerically, with the remainder drawn from
/// `ALPHA / DIGIT / "!" "#" "$" "&" "-" "^" "_" "." "+"` (the narrow
/// restricted-name alphabet, *not* the broader HTTP token set). Keeps arbitrary
/// or non-NFC bytes out of canonical manifests.
pub(crate) fn valid_media_type(s: &str) -> bool {
    /// RFC 6838 §4.2 `restricted-name-chars` (excludes the leading character).
    fn is_restricted_char(b: u8) -> bool {
        b.is_ascii_alphanumeric()
            || matches!(
                b,
                b'!' | b'#' | b'$' | b'&' | b'-' | b'^' | b'_' | b'.' | b'+'
            )
    }
    /// A `restricted-name`: 1..=127 chars, first `ALPHA / DIGIT`, rest restricted.
    fn valid_restricted_name(name: &str) -> bool {
        let bytes = name.as_bytes();
        match bytes.first() {
            Some(&first) if first.is_ascii_alphanumeric() && bytes.len() <= 127 => {
                bytes[1..].iter().all(|&b| is_restricted_char(b))
            }
            _ => false,
        }
    }
    if !s.is_ascii() {
        return false;
    }
    let mut parts = s.split('/');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(t), Some(sub), None) => valid_restricted_name(t) && valid_restricted_name(sub),
        _ => false,
    }
}

/// Retention policy governing which old manifests survive for rollback
/// (Chapter 8 §"Garbage Collection and Retention"). A first-class type
/// (QUICKSTART). The active profile declares it via [`ProfileConstraints`].
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct RetentionPolicy {
    /// Max previous manifests to retain beyond the active one. `0` keeps only
    /// the active manifest (no rollback retention).
    pub retain_previous_manifests: u32,
    /// Optional wall-clock duration after which old retained manifests may be
    /// reclaimed regardless of count.
    pub retain_duration: Option<WallClockDuration>,
    /// Whether to preserve named-checkpoint manifests beyond the limits.
    pub retain_named_checkpoints: bool,
}

impl RetentionPolicy {
    /// A conservative default: keep one previous manifest for rollback, no
    /// time-based eviction, and preserve named checkpoints.
    pub const DEFAULT_FULL: RetentionPolicy = RetentionPolicy {
        retain_previous_manifests: 1,
        retain_duration: None,
        retain_named_checkpoints: true,
    };

    fn encode(&self, w: &mut Writer) {
        w.put_u32(self.retain_previous_manifests);
        w.put_opt(&self.retain_duration, |w, d| d.encode(w));
        w.put_bool(self.retain_named_checkpoints);
    }

    fn decode(r: &mut Reader) -> Result<Self, DecodeError> {
        Ok(RetentionPolicy {
            retain_previous_manifests: r.get_u32()?,
            retain_duration: r.get_opt(WallClockDuration::decode)?,
            retain_named_checkpoints: r.get_bool()?,
        })
    }
}

/// Constraints a profile imposes (Chapter 8 §"Format Profiles"). v0 models the
/// two that matter for the bundle's own validation — the maximum
/// operation-block size and the retention policy — and leaves the richer
/// constraint surface (permitted compression sets, required-extension lists)
/// for later, since those interact with crates beyond Agent D's boundary.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct ProfileConstraints {
    /// Maximum uncompressed operation-envelope block size a reader must accept
    /// (Chapter 8 §"Operation Envelope Blocks", default 64 MiB).
    pub max_uncompressed_block_size: u64,
    /// The retention policy this profile declares.
    pub retention_policy: RetentionPolicy,
}

impl ProfileConstraints {
    /// The Full-profile defaults.
    pub const DEFAULT_FULL: ProfileConstraints = ProfileConstraints {
        max_uncompressed_block_size: crate::block::MAX_BLOCK_DEFAULT,
        retention_policy: RetentionPolicy::DEFAULT_FULL,
    };

    fn encode(&self, w: &mut Writer) {
        w.put_u64(self.max_uncompressed_block_size);
        self.retention_policy.encode(w);
    }

    fn decode(r: &mut Reader) -> Result<Self, DecodeError> {
        Ok(ProfileConstraints {
            max_uncompressed_block_size: r.get_u64()?,
            retention_policy: RetentionPolicy::decode(r)?,
        })
    }
}

/// A conformance-profile declaration (Chapter 8 §"Format Profiles").
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct ProfileDeclaration {
    /// Which profile.
    pub profile_id: ProfileId,
    /// Profile version.
    pub version: SemVer,
    /// Constraints imposed by this profile.
    pub constraints: ProfileConstraints,
}

impl ProfileDeclaration {
    /// The default Full-profile declaration v0 bundles carry.
    pub fn full() -> Self {
        ProfileDeclaration {
            profile_id: ProfileId::Full,
            version: SemVer::new(0, 1, 0),
            constraints: ProfileConstraints::DEFAULT_FULL,
        }
    }

    fn encode(&self, w: &mut Writer) {
        self.profile_id.encode(w);
        self.version.encode(w);
        self.constraints.encode(w);
    }

    fn decode(r: &mut Reader) -> Result<Self, DecodeError> {
        Ok(ProfileDeclaration {
            profile_id: ProfileId::decode(r)?,
            version: SemVer::decode(r)?,
            constraints: ProfileConstraints::decode(r)?,
        })
    }
}

/// An extension declaration (Chapter 8 §"Extension Declarations" / §"Behavior
/// Under Unknown Extensions").
///
/// The bundle's job is *preservation*: it carries the extension's
/// `preserved_chunk_roots` across reads and writes, and refuses editing of a
/// bundle that declares an unknown `required` extension. It does **not**
/// evaluate edit barriers — barrier scopes and prohibited operation kinds are
/// built from `epiphany-core`/`epiphany-ops`/`epiphany-layout-ir` types (the
/// `OperationKindTag`/`ObjectKind`/`EditBarrier` family is owned by Agents C and
/// E). So `affected_object_kinds` and `edit_barriers` are carried here as opaque
/// length-prefixed bytes, preserved verbatim. See `DECISIONS.md`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ExtensionDeclaration {
    /// Extension identity.
    pub extension_id: ExtensionId,
    /// Extension version.
    pub version: SemVer,
    /// If set, a reader that does not understand this extension MUST refuse to
    /// open for editing (but MAY open read-only).
    pub required: bool,
    /// Root chunks owned by this extension; preserved opaquely across writes.
    pub preserved_chunk_roots: Vec<ChunkRef>,
    /// Object kinds this extension affects, for edit-barrier evaluation. Opaque
    /// to the bundle (preserved verbatim).
    pub affected_object_kinds: Vec<u8>,
    /// This extension's edit barriers. Opaque to the bundle (preserved verbatim).
    pub edit_barriers: Vec<u8>,
}

impl ExtensionDeclaration {
    fn encode(&self, w: &mut Writer) {
        self.extension_id.encode(w);
        self.version.encode(w);
        w.put_bool(self.required);
        let roots = sorted_dedup_chunk_refs(&self.preserved_chunk_roots);
        w.put_seq(&roots, |w, c| c.encode(w));
        w.put_var_bytes(&self.affected_object_kinds);
        w.put_var_bytes(&self.edit_barriers);
    }

    fn decode(r: &mut Reader) -> Result<Self, DecodeError> {
        Ok(ExtensionDeclaration {
            extension_id: ExtensionId::decode(r)?,
            version: SemVer::decode(r)?,
            required: r.get_bool()?,
            preserved_chunk_roots: r.get_seq(ChunkRef::decode)?,
            affected_object_kinds: r.get_var_bytes()?,
            edit_barriers: r.get_var_bytes()?,
        })
    }
}

/// The manifest itself.
/// Summary metadata for one operation-envelope block (Chapter 8: an
/// `OperationEnvelopeBlock`'s `dvv_summary` / `min_stamp` / `max_stamp`).
///
/// These are *semantic* values — a causal frontier (DVV) and operation-stamp
/// range computed by reading the block's envelopes — which belong to the
/// operation layer (Agent C), not the bundle. The bundle treats them as **opaque
/// bytes**, computed and interpreted by `epiphany-ops`, and carries them keyed by
/// the block's chunk id so a reader can select or skip a block by causal frontier
/// or stamp range **without decoding its envelopes** (the point of a summary).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct OperationBlockSummary {
    /// The causal frontier (DVV) the block covers — opaque ops-computed bytes.
    pub dvv_summary: FrontierBytes,
    /// The block's minimum operation stamp, canonical bytes — opaque to the bundle.
    pub min_stamp: Vec<u8>,
    /// The block's maximum operation stamp, canonical bytes — opaque to the bundle.
    pub max_stamp: Vec<u8>,
}

impl OperationBlockSummary {
    fn encode(&self, w: &mut Writer) {
        self.dvv_summary.encode(w);
        w.put_var_bytes(&self.min_stamp);
        w.put_var_bytes(&self.max_stamp);
    }
    fn decode(r: &mut Reader) -> Result<Self, DecodeError> {
        Ok(OperationBlockSummary {
            dvv_summary: FrontierBytes::decode(r)?,
            min_stamp: r.get_var_bytes()?,
            max_stamp: r.get_var_bytes()?,
        })
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Manifest {
    /// Logical-work identity.
    pub document_id: DocumentId,
    /// Optional shared-ancestor identity.
    pub lineage_id: Option<LineageId>,
    /// Identity of this manifest. Derived from content at encode time (each
    /// commit produces a new id); see [`Manifest::derive_id`].
    pub manifest_id: ManifestId,
    /// Generation, matching the referencing superblock.
    pub generation: u64,
    /// Operation-envelope blocks defining the canonical document (canonical
    /// root).
    pub operation_roots: Vec<ChunkRef>,
    /// Per-operation-block summaries (Chapter 8: `dvv_summary`/`min_stamp`/
    /// `max_stamp`), keyed by the block's chunk id. Opaque, ops-supplied metadata
    /// that lets a reader select blocks without decoding them; non-canonical and
    /// optional (a block need not have an entry).
    pub operation_block_summaries: BTreeMap<ChunkId, OperationBlockSummary>,
    /// Optional operation index (non-canonical accelerator).
    pub operation_index_root: Option<ChunkRef>,
    /// The active canonical base snapshot, if pruning has occurred (canonical
    /// root). At most one is active at a time.
    pub canonical_base: Option<SnapshotRef>,
    /// Acceleration snapshots: caches at frontiers other than the base
    /// (non-canonical). Distinct from `canonical_base`.
    pub acceleration_snapshots: Vec<SnapshotRef>,
    /// Blob references.
    pub blob_roots: Vec<BlobRef>,
    /// Profile declarations (at least one required).
    pub profile_declarations: Vec<ProfileDeclaration>,
    /// Extension declarations.
    pub extension_declarations: Vec<ExtensionDeclaration>,
    /// Text projection root (non-canonical accelerator), if maintained.
    pub text_projection_root: Option<ChunkRef>,
    /// Integrity index root (non-canonical accelerator), if maintained.
    pub integrity_root: Option<ChunkRef>,
}

impl Manifest {
    /// A minimal manifest for a freshly created, empty bundle: the given
    /// document id, generation 0, a single Full-profile declaration, and no
    /// roots. The `manifest_id` is filled in at encode time.
    pub fn empty(document_id: DocumentId) -> Self {
        Manifest {
            document_id,
            lineage_id: None,
            manifest_id: ManifestId::default(),
            generation: 0,
            operation_roots: Vec::new(),
            operation_block_summaries: BTreeMap::new(),
            operation_index_root: None,
            canonical_base: None,
            acceleration_snapshots: Vec::new(),
            blob_roots: Vec::new(),
            profile_declarations: vec![ProfileDeclaration::full()],
            extension_declarations: Vec::new(),
            text_projection_root: None,
            integrity_root: None,
        }
    }

    /// The profile declarations in *canonical* order (the order the manifest
    /// serializes in), deduplicated. Profile selection iterates this so the
    /// choice is stable across serialization — the in-memory and reloaded orders
    /// agree.
    pub fn canonical_profiles(&self) -> Vec<ProfileDeclaration> {
        sorted_dedup_by_key(
            &self.profile_declarations,
            |p| (p.profile_id, p.version),
            |w, p| p.encode(w),
        )
    }

    /// The summary recorded for the given operation block, if any. Lets a reader
    /// select or skip a block by causal frontier / stamp range without decoding
    /// its envelopes (Chapter 8: `OperationEnvelopeBlock` summary metadata).
    pub fn operation_block_summary(&self, block: ChunkId) -> Option<&OperationBlockSummary> {
        self.operation_block_summaries.get(&block)
    }

    /// The first profile declaration in canonical order, or `None` if none is
    /// declared.
    pub fn canonical_first_profile(&self) -> Option<ProfileDeclaration> {
        self.canonical_profiles().into_iter().next()
    }

    /// The retention policy of the canonical-first declared profile, or the Full
    /// default if (against the spec) none is declared.
    pub fn retention_policy(&self) -> RetentionPolicy {
        self.canonical_first_profile()
            .map(|p| p.constraints.retention_policy)
            .unwrap_or(RetentionPolicy::DEFAULT_FULL)
    }

    /// Encodes the identity-bearing body: every field *except* `manifest_id`,
    /// with vectors put in canonical order. This is both the manifest-id
    /// preimage and the second half of the on-disk encoding.
    fn encode_body(&self) -> Vec<u8> {
        let mut w = Writer::new();
        self.document_id.encode(&mut w);
        w.put_opt(&self.lineage_id, |w, l| l.encode(w));
        w.put_u64(self.generation);

        // Chunk-reference vectors use the Appendix D order — `ChunkKind`
        // discriminant, then content hash, then offset — which is `ChunkRef`'s
        // own `Ord`; the others use a deterministic total order by full encoded
        // bytes. Every vector is deduplicated: these fields are sets (e.g. the
        // envelope set is a union), so one root and two identical roots must
        // canonicalize to the same bytes.
        let op_roots = sorted_dedup_chunk_refs(&self.operation_roots);
        w.put_seq(&op_roots, |w, c| c.encode(w));

        // Per-block summaries, in canonical (BTreeMap = ChunkId-ascending) order.
        let summaries: Vec<(&ChunkId, &OperationBlockSummary)> =
            self.operation_block_summaries.iter().collect();
        w.put_seq(&summaries, |w, entry| {
            w.put_bytes(entry.0.as_bytes());
            entry.1.encode(w);
        });

        w.put_opt(&self.operation_index_root, |w, c| c.encode(w));
        w.put_opt(&self.canonical_base, |w, s| s.encode(w));

        let accel = sorted_dedup_by_encoding(&self.acceleration_snapshots, |w, s| s.encode(w));
        w.put_seq(&accel, |w, s| s.encode(w));

        let blobs = sorted_dedup_by_encoding(&self.blob_roots, |w, b| b.encode(w));
        w.put_seq(&blobs, |w, b| b.encode(w));

        // Profiles and extension declarations carry a `SemVer`, which must order
        // *numerically* (Appendix D §"Ordered Iteration": extensions ascend by
        // id then by semantic version). Sorting by encoded bytes would order the
        // little-endian version integers byte-wise — putting 256.0.0 before
        // 1.0.0 — so these sort by an explicit `(id, version)` key.
        let profiles = sorted_dedup_by_key(
            &self.profile_declarations,
            |p| (p.profile_id, p.version),
            |w, p| p.encode(w),
        );
        w.put_seq(&profiles, |w, p| p.encode(w));

        let exts = sorted_dedup_by_key(
            &self.extension_declarations,
            |e| (e.extension_id, e.version),
            |w, e| e.encode(w),
        );
        w.put_seq(&exts, |w, e| e.encode(w));

        w.put_opt(&self.text_projection_root, |w, c| c.encode(w));
        w.put_opt(&self.integrity_root, |w, c| c.encode(w));
        w.into_bytes()
    }

    /// The content-derived manifest id for this manifest's current content.
    pub fn derive_id(&self) -> ManifestId {
        ManifestId::derive(self.document_id, self.generation, &self.encode_body())
    }

    /// Encodes the manifest to its canonical chunk payload. The `manifest_id` is
    /// (re)derived from the body and written first, so the encoding is
    /// self-consistent and byte-stable regardless of the in-memory id field.
    pub fn encode(&self) -> Vec<u8> {
        let body = self.encode_body();
        let id = ManifestId::derive(self.document_id, self.generation, &body);
        let mut w = Writer::with_capacity(16 + body.len());
        id.encode(&mut w);
        w.put_bytes(&body);
        w.into_bytes()
    }

    /// Decodes a manifest from its canonical chunk payload, verifying that the
    /// stored `manifest_id` matches the id derived from the body (a corrupt or
    /// foreign manifest fails here, in addition to the chunk-hash check the
    /// caller already performed against the superblock).
    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        let mut r = Reader::new(bytes);
        let stored_id = ManifestId::decode(&mut r)?;
        let document_id = DocumentId::decode(&mut r)?;
        let lineage_id = r.get_opt(LineageId::decode)?;
        let generation = r.get_u64()?;
        let operation_roots = r.get_seq(ChunkRef::decode)?;
        let summary_entries = r.get_seq(|r| {
            let id = ChunkId(ContentHash(r.take_array::<32>()?));
            let summary = OperationBlockSummary::decode(r)?;
            Ok((id, summary))
        })?;
        let operation_block_summaries: BTreeMap<ChunkId, OperationBlockSummary> =
            summary_entries.into_iter().collect();
        let operation_index_root = r.get_opt(ChunkRef::decode)?;
        let canonical_base = r.get_opt(SnapshotRef::decode)?;
        let acceleration_snapshots = r.get_seq(SnapshotRef::decode)?;
        let blob_roots = r.get_seq(BlobRef::decode)?;
        let profile_declarations = r.get_seq(ProfileDeclaration::decode)?;
        let extension_declarations = r.get_seq(ExtensionDeclaration::decode)?;
        let text_projection_root = r.get_opt(ChunkRef::decode)?;
        let integrity_root = r.get_opt(ChunkRef::decode)?;
        r.finish()?;

        let manifest = Manifest {
            document_id,
            lineage_id,
            manifest_id: stored_id,
            generation,
            operation_roots,
            operation_block_summaries,
            operation_index_root,
            canonical_base,
            acceleration_snapshots,
            blob_roots,
            profile_declarations,
            extension_declarations,
            text_projection_root,
            integrity_root,
        };
        // Accept only *canonical* manifest bytes: re-encoding the decoded
        // manifest must reproduce the input exactly. This is a single total
        // check that subsumes (a) the `manifest_id` matching the content
        // (`encode` re-derives it), and (b) every vector being in canonical
        // order with no order-dependent duplicates — so `decode ∘ encode` is the
        // identity and accepted bytes are guaranteed byte-stable.
        if manifest.encode() != bytes {
            return Err(DecodeError::Malformed(
                "non-canonical manifest encoding (unsorted, duplicated, or wrong manifest id)",
            ));
        }
        Ok(manifest)
    }

    /// The schema version manifests are encoded against in this crate.
    pub const SCHEMA: SchemaVersion = SchemaVersion::V0;

    /// The *chunk-typed* canonical roots: the operation blocks and the canonical
    /// base's root chunk. (Canonical blobs are referenced via [`BlobRef`], not
    /// [`ChunkRef`], and the bundle cannot tell which blobs are canonical without
    /// interpreting operations, so blob verification is handled separately by
    /// [`crate::Bundle::verify_canonical_chunks`].)
    pub fn canonical_chunk_refs(&self) -> Vec<ChunkRef> {
        let mut refs: Vec<ChunkRef> = Vec::new();
        refs.extend(self.operation_roots.iter().copied());
        if let Some(base) = &self.canonical_base {
            refs.push(base.root);
        }
        refs
    }

    /// Every chunk *reference* the manifest holds (canonical or not). Used by
    /// commit to deduplicate: a newly-staged chunk whose content hash already
    /// appears here reuses the existing storage rather than re-appending
    /// (Chapter 8 §"Chunks": *"Duplicate content shares storage automatically"*).
    pub fn referenced_chunk_refs(&self) -> Vec<ChunkRef> {
        let mut refs: Vec<ChunkRef> = Vec::new();
        refs.extend(self.operation_roots.iter().copied());
        refs.extend(self.operation_index_root);
        if let Some(b) = &self.canonical_base {
            refs.push(b.root);
        }
        for s in &self.acceleration_snapshots {
            refs.push(s.root);
        }
        for e in &self.extension_declarations {
            refs.extend(e.preserved_chunk_roots.iter().copied());
        }
        refs.extend(self.text_projection_root);
        refs.extend(self.integrity_root);
        refs
    }

    /// Every chunk id the manifest references, canonical or not, for
    /// reachability/garbage-collection reasoning (Chapter 8 §"Garbage
    /// Collection and Retention").
    pub fn all_chunk_ids(&self) -> Vec<ChunkId> {
        let mut ids: Vec<ChunkId> = Vec::new();
        let mut push = |c: &ChunkRef| ids.push(c.id);
        self.operation_roots.iter().for_each(&mut push);
        self.operation_index_root.iter().for_each(&mut push);
        if let Some(b) = &self.canonical_base {
            push(&b.root);
        }
        for s in &self.acceleration_snapshots {
            push(&s.root);
        }
        for e in &self.extension_declarations {
            e.preserved_chunk_roots.iter().for_each(&mut push);
        }
        self.text_projection_root.iter().for_each(&mut push);
        self.integrity_root.iter().for_each(&mut push);
        ids
    }
}

/// Returns a copy of `items` sorted ascending by each element's **full encoded
/// bytes**, with duplicates removed. Used to put manifest vectors into a
/// canonical *total* order at encode time (Appendix D §"Ordered Iteration"), so
/// re-encoding is byte-stable regardless of in-memory insertion order, ties on a
/// partial key have a deterministic order, and a set-like field with an
/// accidental duplicate canonicalizes to a single copy.
fn sorted_dedup_by_encoding<T: Clone>(items: &[T], encode: impl Fn(&mut Writer, &T)) -> Vec<T> {
    let mut keyed: Vec<(Vec<u8>, T)> = items
        .iter()
        .map(|t| {
            let mut w = Writer::new();
            encode(&mut w, t);
            (w.into_bytes(), t.clone())
        })
        .collect();
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    keyed.dedup_by(|a, b| a.0 == b.0);
    keyed.into_iter().map(|(_, t)| t).collect()
}

/// Returns a copy of `items` sorted by an explicit `Ord` key, then by full
/// encoded bytes as a tie-break, with duplicates removed. Used where the
/// canonical order is *not* the encoded-byte order — notably anything carrying a
/// `SemVer`, whose little-endian integers must order numerically, not byte-wise.
fn sorted_dedup_by_key<T: Clone, K: Ord>(
    items: &[T],
    key: impl Fn(&T) -> K,
    encode: impl Fn(&mut Writer, &T),
) -> Vec<T> {
    let mut keyed: Vec<(K, Vec<u8>, T)> = items
        .iter()
        .map(|t| {
            let mut w = Writer::new();
            encode(&mut w, t);
            (key(t), w.into_bytes(), t.clone())
        })
        .collect();
    keyed.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    keyed.dedup_by(|a, b| a.1 == b.1);
    keyed.into_iter().map(|(_, _, t)| t).collect()
}

/// Returns a copy of `refs` in the Appendix D chunk-reference order —
/// `ChunkKind` discriminant, then content hash, then offset (which is
/// [`ChunkRef`]'s `Ord`) — with exact duplicates removed. (Full-byte sorting is
/// wrong here: it would order by the leading `id`/`hash` field before the kind,
/// mis-ordering a mixed-kind vector.)
fn sorted_dedup_chunk_refs(refs: &[ChunkRef]) -> Vec<ChunkRef> {
    let mut v = refs.to_vec();
    v.sort();
    v.dedup();
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::{chunk_id, ChunkKind};
    use crate::ids::SnapshotId;

    /// Exhaustive over **every single-byte replacement of this constructed
    /// manifest**: each of its bytes, each of the 255 other values. All rejected.
    ///
    /// That is a finite check, not a proof of totality. Totality rests on the
    /// *argument*: `manifest_id` is derived from the body, so a body edit fails
    /// the id check and an id edit fails the derivation; and `encode_body` sorts
    /// and deduplicates every vector, so an out-of-order or duplicated encoding
    /// cannot round-trip. The test is evidence for that argument over one
    /// manifest, not a substitute for it — and multi-byte perturbations are out
    /// of its reach entirely.
    ///
    /// The argument is what makes the guard complete *here* and not in
    /// `MaterializedState` (epiphany-ops), whose encoder writes its `Vec` fields
    /// verbatim, nor in `OperationIndex`, which has no guard at all. The lenient
    /// `CompressionAlgorithm::None` parameter byte was invisible through the
    /// manifest for exactly this reason, and visible through the index.
    #[test]
    fn every_single_byte_replacement_of_a_manifest_is_rejected() {
        use crate::chunk::{ChunkRef, CompressionAlgorithm};
        use crate::ids::SchemaVersion;

        let mut m = Manifest::empty(DocumentId([5; 16]));
        m.operation_roots.push(ChunkRef {
            id: ChunkId(ContentHash([0x11; 32])),
            kind: ChunkKind::OperationEnvelopeBlock,
            schema_version: SchemaVersion::V0,
            offset: 576,
            compressed_length: 64,
            uncompressed_length: 64,
            compression: CompressionAlgorithm::None,
            hash: ContentHash([0x11; 32]),
        });
        let bytes = m.encode();
        // `encode` re-derives `manifest_id` from the body, so the decoded
        // manifest carries the derived id while `m` still holds the empty one.
        // Compare the canonical bytes, which is what the guard compares.
        assert_eq!(Manifest::decode(&bytes).unwrap().encode(), bytes);

        for i in 0..bytes.len() {
            let original = bytes[i];
            for value in 0u16..=255 {
                let value = value as u8;
                if value == original {
                    continue;
                }
                let mut b = bytes.clone();
                b[i] = value;
                match Manifest::decode(&b) {
                    Err(_) => {}
                    Ok(decoded) => panic!(
                        "byte {i} = {value:#04x} (was {original:#04x}) was accepted; \
                         re-encode matches input: {}",
                        decoded.encode() == b
                    ),
                }
            }
        }
    }

    #[test]
    fn operation_block_summaries_round_trip_and_are_selectable() {
        let mut m = Manifest::empty(DocumentId([5; 16]));
        let block = ChunkId(ContentHash([7; 32]));
        m.operation_block_summaries.insert(
            block,
            OperationBlockSummary {
                dvv_summary: FrontierBytes::from_bytes(vec![1, 2, 3]),
                min_stamp: vec![10, 11],
                max_stamp: vec![20, 21],
            },
        );
        // The summary survives the canonical encode/decode of the manifest...
        let decoded = Manifest::decode(&m.encode()).expect("manifest decodes");
        let summary = decoded
            .operation_block_summary(block)
            .expect("summary preserved");
        assert_eq!(summary.dvv_summary.as_bytes(), &[1, 2, 3]);
        assert_eq!(summary.min_stamp, vec![10, 11]);
        assert_eq!(summary.max_stamp, vec![20, 21]);
        // ...and a reader selects by block id without touching any block payload.
        assert!(decoded
            .operation_block_summary(ChunkId(ContentHash([8; 32])))
            .is_none());
    }

    #[test]
    fn semver_orders_numerically_not_byte_wise() {
        // SemVer integers are little-endian; a byte-order sort would put 256.0.0
        // before 1.0.0. The canonical order must be numeric.
        let ext = |major| ExtensionDeclaration {
            extension_id: ExtensionId([1; 16]),
            version: SemVer::new(major, 0, 0),
            required: false,
            preserved_chunk_roots: Vec::new(),
            affected_object_kinds: Vec::new(),
            edit_barriers: Vec::new(),
        };
        let mut m = Manifest::empty(DocumentId([1; 16]));
        m.extension_declarations = vec![ext(256), ext(1)];
        let decoded = Manifest::decode(&m.encode()).unwrap();
        let majors: Vec<u32> = decoded
            .extension_declarations
            .iter()
            .map(|e| e.version.major)
            .collect();
        assert_eq!(majors, vec![1, 256]);
    }

    #[test]
    fn blob_media_type_is_validated() {
        assert!(valid_media_type("audio/wav"));
        assert!(valid_media_type("application/octet-stream"));
        assert!(valid_media_type("application/vnd.api+json"));
        assert!(valid_media_type("1/2")); // digit-first is allowed
        assert!(!valid_media_type("audiowav")); // no slash
        assert!(!valid_media_type("audio/")); // empty subtype
        assert!(!valid_media_type("audio/wav/extra")); // two slashes
        assert!(!valid_media_type("audio/wáv")); // non-ASCII
        assert!(!valid_media_type("au dio/wav")); // space not a token char
                                                  // RFC 6838 restricted-name alphabet excludes these (HTTP-token chars):
        assert!(!valid_media_type("x%y/z"));
        assert!(!valid_media_type("a/b*c"));
        assert!(!valid_media_type("a'b/c"));
        assert!(!valid_media_type("a/b~c"));
        assert!(!valid_media_type("a|b/c"));
        // Must begin alphanumerically, not with punctuation.
        assert!(!valid_media_type(".foo/bar"));
        assert!(!valid_media_type("foo/-bar"));
        // Components are limited to 127 characters.
        let long = format!("a{}/b", "a".repeat(127));
        assert!(!valid_media_type(&long));
        assert!(valid_media_type(&format!("{}/b", "a".repeat(127))));

        // A manifest carrying an invalid media type fails to decode.
        let mut m = Manifest::empty(DocumentId([1; 16]));
        m.blob_roots.push(BlobRef {
            blob_id: BlobId::of_payload(b"x"),
            media_type: "not a media type".to_string(),
            offset: 600,
            compressed_length: 1,
            uncompressed_length: 1,
            compression: CompressionAlgorithm::None,
            hash: BlobId::of_payload(b"x").0,
            declared_max_uncompressed_length: None,
        });
        assert!(matches!(
            Manifest::decode(&m.encode()),
            Err(DecodeError::Malformed(_))
        ));
    }

    fn chunk_ref(kind: ChunkKind, payload: &[u8], offset: u64) -> ChunkRef {
        let id = chunk_id(kind, SchemaVersion::V0, payload);
        ChunkRef {
            id,
            kind,
            schema_version: SchemaVersion::V0,
            offset,
            compressed_length: payload.len() as u64,
            uncompressed_length: payload.len() as u64,
            compression: CompressionAlgorithm::None,
            hash: id.content_hash(),
        }
    }

    fn chunk_ref_with_hash(kind: ChunkKind, hash_byte: u8) -> ChunkRef {
        let h = crate::ContentHash([hash_byte; 32]);
        ChunkRef {
            id: crate::ChunkId(h),
            kind,
            schema_version: SchemaVersion::V0,
            offset: 600,
            compressed_length: 1,
            uncompressed_length: 1,
            compression: CompressionAlgorithm::None,
            hash: h,
        }
    }

    #[test]
    fn chunk_refs_sort_by_kind_before_hash() {
        // Appendix D order: ChunkKind discriminant, THEN content hash. A Snapshot
        // (disc 2) with a low hash must still sort after an OperationEnvelopeBlock
        // (disc 0) with a high hash — full-byte sorting (id/hash first) would get
        // this wrong.
        let snap = chunk_ref_with_hash(ChunkKind::Snapshot, 0x01);
        let op = chunk_ref_with_hash(ChunkKind::OperationEnvelopeBlock, 0xFF);
        let mut m = Manifest::empty(DocumentId([1; 16]));
        m.extension_declarations = vec![ExtensionDeclaration {
            extension_id: ExtensionId([1; 16]),
            version: SemVer::new(1, 0, 0),
            required: false,
            preserved_chunk_roots: vec![snap, op],
            affected_object_kinds: Vec::new(),
            edit_barriers: Vec::new(),
        }];
        let decoded = Manifest::decode(&m.encode()).unwrap();
        let roots = &decoded.extension_declarations[0].preserved_chunk_roots;
        assert_eq!(roots[0].kind, ChunkKind::OperationEnvelopeBlock);
        assert_eq!(roots[1].kind, ChunkKind::Snapshot);
    }

    #[test]
    fn duplicate_roots_collapse_on_encode() {
        // Operation roots are a set (envelope blocks form a union): one root and
        // two identical roots must canonicalize to the same bytes and id.
        let r = chunk_ref(ChunkKind::OperationEnvelopeBlock, b"block", 600);
        let mut one = Manifest::empty(DocumentId([1; 16]));
        one.operation_roots = vec![r];
        let mut two = one.clone();
        two.operation_roots = vec![r, r];
        assert_eq!(one.encode(), two.encode());
        assert_eq!(one.derive_id(), two.derive_id());
    }

    fn rich_manifest() -> Manifest {
        let mut m = Manifest::empty(DocumentId([1; 16]));
        m.generation = 4;
        m.lineage_id = Some(LineageId([2; 16]));
        m.operation_roots = vec![
            chunk_ref(ChunkKind::OperationEnvelopeBlock, b"block-b", 600),
            chunk_ref(ChunkKind::OperationEnvelopeBlock, b"block-a", 700),
        ];
        m.canonical_base = Some(SnapshotRef {
            snapshot_id: SnapshotId([9; 16]),
            covers_causal_frontier: FrontierBytes::from_bytes(vec![1, 2, 3]),
            reduction_algorithm_version: ReductionAlgorithmVersion(1),
            profile_id: ProfileId::Full,
            root: chunk_ref(ChunkKind::Snapshot, b"snap", 800),
            hash: chunk_id(ChunkKind::Snapshot, SchemaVersion::V0, b"snap").content_hash(),
        });
        m.blob_roots = vec![BlobRef {
            blob_id: BlobId::of_payload(b"audio"),
            media_type: "audio/wav".to_string(),
            offset: 900,
            compressed_length: 5,
            uncompressed_length: 5,
            compression: CompressionAlgorithm::None,
            hash: BlobId::of_payload(b"audio").0,
            declared_max_uncompressed_length: Some(1 << 20),
        }];
        m.extension_declarations = vec![ExtensionDeclaration {
            extension_id: ExtensionId([7; 16]),
            version: SemVer::new(1, 0, 0),
            required: false,
            preserved_chunk_roots: vec![chunk_ref(ChunkKind::ExtensionData, b"ext", 1000)],
            affected_object_kinds: vec![0xAA, 0xBB],
            edit_barriers: vec![0xCC],
        }];
        m
    }

    #[test]
    fn manifest_round_trips() {
        let m = rich_manifest();
        let bytes = m.encode();
        let decoded = Manifest::decode(&bytes).unwrap();
        // The decoded manifest carries the derived id; compare with that filled in.
        let mut expected = m.clone();
        expected.manifest_id = m.derive_id();
        // Vectors are canonicalized on encode; compare via re-encode for equality.
        assert_eq!(decoded.encode(), bytes);
        assert_eq!(decoded.document_id, expected.document_id);
        assert_eq!(decoded.canonical_base, expected.canonical_base);
        assert_eq!(decoded.manifest_id, expected.manifest_id);
    }

    #[test]
    fn re_encode_is_byte_identical() {
        // v0 acceptance criterion 4 (canonical serialization stability) at the
        // manifest level: serialize -> load -> re-serialize is byte-identical.
        let bytes = rich_manifest().encode();
        let reloaded = Manifest::decode(&bytes).unwrap();
        assert_eq!(reloaded.encode(), bytes);
    }

    #[test]
    fn encoding_is_insertion_order_independent() {
        let mut a = Manifest::empty(DocumentId([3; 16]));
        let r1 = chunk_ref(ChunkKind::OperationEnvelopeBlock, b"one", 600);
        let r2 = chunk_ref(ChunkKind::OperationEnvelopeBlock, b"two", 700);
        a.operation_roots = vec![r1, r2];
        let mut b = a.clone();
        b.operation_roots = vec![r2, r1];
        assert_eq!(
            a.encode(),
            b.encode(),
            "canonical order absorbs insertion order"
        );
    }

    #[test]
    fn manifest_id_changes_with_content() {
        let m1 = rich_manifest();
        let mut m2 = m1.clone();
        m2.generation = 5;
        assert_ne!(m1.derive_id(), m2.derive_id());
    }

    #[test]
    fn tampered_manifest_id_is_rejected() {
        let mut bytes = rich_manifest().encode();
        // Flip a byte inside the leading manifest-id field.
        bytes[0] ^= 0xFF;
        assert!(matches!(
            Manifest::decode(&bytes),
            Err(DecodeError::Malformed(_))
        ));
    }

    #[test]
    fn canonical_refs_cover_operation_roots_and_base() {
        let m = rich_manifest();
        let canonical = m.canonical_chunk_refs();
        assert_eq!(canonical.len(), 3); // 2 op blocks + 1 base root
    }
}
