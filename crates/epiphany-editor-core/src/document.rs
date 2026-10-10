//! The document layer: an [`EditorDocument`] owns a bundle and the operations
//! committed to it, and leases one writable [`EditorSession`] at a time
//! (`spec/PLAN_EDITOR_APP.md` §Ruling B; T1b).
//!
//! A document is `Score::empty(identity)` plus its envelope log
//! (`spec/RULING_GENESIS_PERSISTENCE.md`): every envelope in the bundle's
//! operation blocks, reduced from an empty score in graph-aware mode. Nothing
//! else is stored — no base, no snapshot, no frontier — so opening is a full
//! replay, and the frontier a new edit extends is derived from the committed
//! membership ([`frontier_of`]).
//!
//! Ownership follows Ruling B and the Ruling D condition. The document owns the
//! bundle, the committed operation set and its generation, the read-only status
//! and the lease; the session owns undo, dirty state, selection, the caret, the
//! solver and the layout. [`EditorDocument::lease`] grants a session carrying a
//! private [`Lease`] (the document instance, a lease number and the committed
//! generation), and [`EditorDocument::save`] refuses any session whose lease is
//! not the document's current one: a probe-mode session, one leased from
//! another document, or one a later lease revoked. Save commits exactly the
//! session's applied units as new operation blocks and, only after the commit
//! point's durable flush, promotes them into the committed partition, so undo
//! stops at the save.
//!
//! A commit has three outcomes. One that fails before its commit point leaves
//! the units unsaved and undoable ([`DocumentError::Bundle`]). One that
//! succeeds promotes them ([`Saved`]). One whose commit-point flush fails is
//! indeterminate: the bundle poisons itself read-only, the units stay unsaved,
//! and [`EditorDocument::reconcile`] reopens the store, checks whether the
//! staged blocks are reachable and, only after a fresh durability barrier
//! succeeds, reports them saved.
//!
//! Reopen reads what the current writer writes. An envelope in a recognized
//! historical layout is refused by name
//! ([`DocumentError::UnsupportedHistoricalEncoding`], Ruling B's interim); the
//! general boundary on mixed-major files is a later phase's.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use epiphany_bundle::{
    chunk_id, decode_block, pack_operation_blocks, BlockStore, Bundle, BundleCapabilities,
    BundleError, ChunkId, ChunkKind, CommitContext, DocumentId, FileUuid, IntegrityAnomaly,
    Manifest, ReductionAlgorithmVersion, SchemaVersion, StagedChunk,
};
use epiphany_core::{
    Clef, IdentityContext, Instrument, KeySignature, KeySignatureChange, Measure,
    MeasureNumberVisibility, MetricTimeModel, MusicalDuration, OperationId, PowerOfTwo,
    RationalTime, Region, RegionContent, RegionEdge, RegionTimeModel, ReplicaId, Score,
    ScoreMetadata, Staff, StaffExtent, StaffInstance, StaffLineConfiguration, TimeAnchor,
    TimeExtent, TimeSignature, TimeSignatureDisplay, Timestamp, Voice, VoiceOrigin, WallClockTime,
};
use epiphany_determinism::CanonicalEncode;
use epiphany_layout_ir::ConstraintSolver;
use epiphany_ops::{
    decode_envelope, operation_block_introduced_minor, AuthorId, CausalContext, CreateInstrumentOp,
    CreateMeasureOp, CreateRegionOp, CreateStaffInstanceOp, CreateStaffOp, CreateVoiceOp,
    EnvelopeDecodeError, HybridLogicalClock, OperationEnvelope, OperationKind, OperationPayload,
    OperationSet, OperationStamp, SetMetadataOp, SetTimeSignatureOp,
    CURRENT_REDUCTION_ALGORITHM_VERSION,
};

use crate::{EditorError, EditorSession};

/// The capabilities a document states when it opens or creates a bundle: the
/// reduction semantics this build implements (`docs/invariants.md`, production
/// versus fixture capabilities).
fn production_caps() -> BundleCapabilities {
    BundleCapabilities {
        current_reduction_version: ReductionAlgorithmVersion(CURRENT_REDUCTION_ALGORITHM_VERSION),
    }
}

/// Distinguishes document instances within a process, so a lease from one open
/// of a file cannot save into another open of the same file.
static NEXT_INSTANCE: AtomicU64 = AtomicU64::new(1);

/// The private token a document grants with a session: which document instance
/// granted it, which of that document's leases it is, and the committed
/// generation the session's partition reflects. Only this module constructs one,
/// so no caller can forge a savable session.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) struct Lease {
    instance: u64,
    number: u64,
    generation: u64,
}

/// The committed partition a session reduces under its own edits: the envelopes,
/// the operation set they make (accepted once, so an edit's materialization does
/// not re-hash them), and the frontier derived from its membership. Shared
/// between the document and its session; never mutated, only replaced at a save.
#[derive(Clone, Default)]
pub(crate) struct Committed {
    envelopes: Arc<Vec<OperationEnvelope>>,
    set: Arc<OperationSet>,
    frontier: CausalContext,
}

impl Committed {
    /// The partition holding `envelopes`. Identical duplicates (one envelope in two
    /// blocks) are accepted as set duplicates; a second distinct envelope under one
    /// id equivocates the slot, which the reduction then reports.
    fn new(envelopes: Vec<OperationEnvelope>) -> Self {
        let mut set = OperationSet::new();
        for envelope in &envelopes {
            set.accept(envelope.clone());
        }
        let frontier = frontier_of(set.slots().map(|(id, _)| *id));
        Committed {
            envelopes: Arc::new(envelopes),
            set: Arc::new(set),
            frontier,
        }
    }

    /// This partition with `promoted` appended.
    fn extended(&self, promoted: &[OperationEnvelope]) -> Self {
        let mut envelopes = Vec::with_capacity(self.envelopes.len() + promoted.len());
        envelopes.extend(self.envelopes.iter().cloned());
        envelopes.extend(promoted.iter().cloned());
        let mut set = (*self.set).clone();
        for envelope in promoted {
            set.accept(envelope.clone());
        }
        let frontier = frontier_of(set.slots().map(|(id, _)| *id));
        Committed {
            envelopes: Arc::new(envelopes),
            set: Arc::new(set),
            frontier,
        }
    }

    pub(crate) fn envelopes(&self) -> &[OperationEnvelope] {
        &self.envelopes
    }

    pub(crate) fn set(&self) -> &OperationSet {
        &self.set
    }

    pub(crate) fn frontier(&self) -> &CausalContext {
        &self.frontier
    }

    /// One past the highest operation counter `replica` holds in this partition, or
    /// 0 when it holds none: where a session minting under `replica` starts.
    pub(crate) fn next_counter(&self, replica: ReplicaId) -> u64 {
        self.set
            .slots()
            .map(|(id, _)| *id)
            .filter(|id| id.replica == replica)
            .map(|id| id.counter + 1)
            .max()
            .unwrap_or(0)
    }
}

/// The causal context exactly covering `ids`, honoring the dotted-version-vector
/// model (`epiphany-ops`' `causal.rs`): per replica, the vector floor covers only
/// the contiguous counter prefix from 0, and every operation beyond a gap is an
/// individual dot. Claiming a floor past a gap would assert predecessors the set
/// does not hold, and the reducer would hold the new edit pending behind them.
pub(crate) fn frontier_of(ids: impl IntoIterator<Item = OperationId>) -> CausalContext {
    let mut by_replica: std::collections::BTreeMap<ReplicaId, Vec<u64>> =
        std::collections::BTreeMap::new();
    for id in ids {
        by_replica.entry(id.replica).or_default().push(id.counter);
    }
    let mut context = CausalContext::new();
    for (replica, mut counters) in by_replica {
        counters.sort_unstable();
        counters.dedup();
        let contiguous = counters
            .iter()
            .enumerate()
            .take_while(|(i, c)| **c == *i as u64)
            .count();
        if contiguous > 0 {
            context = context.with_seen(replica, contiguous as u64 - 1);
        }
        for &counter in &counters[contiguous..] {
            context = context.with_dot(OperationId::new(replica, counter));
        }
    }
    context
}

/// Why a document opened read-only: it can be viewed ([`EditorDocument::view`])
/// but grants no lease, so nothing is saved into it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ReadOnlyReason {
    /// The bundle itself opened read-only (an anomaly from crash recovery, a
    /// profile this build does not edit, or a required extension it does not
    /// understand), with the anomalies it recorded.
    Bundle {
        /// What the bundle recorded on opening.
        anomalies: Vec<IntegrityAnomaly>,
    },
    /// The manifest declares extensions. Editing an extension-bearing document
    /// waits for the format's tombstone channel (Ruling B: extensions fail closed).
    Extensions,
    /// The committed operations do not reduce cleanly: a semantic conflict, an
    /// operation held pending on a missing predecessor, or an anomaly such as an
    /// equivocation. The document is understood and drawn, but not edited, until
    /// collaboration support can resolve it.
    Unclean {
        /// Unresolved conflicts.
        conflicts: usize,
        /// Operations held pending.
        pending: usize,
        /// Reduction anomalies.
        anomalies: usize,
    },
}

/// Why a document could not be created, opened, leased or saved.
#[derive(Debug)]
pub enum DocumentError {
    /// The bundle refused: corrupt bytes on opening, an I/O failure, or a commit
    /// that failed before its commit point (whose units stay unsaved and
    /// undoable).
    Bundle(BundleError),
    /// A canonical operation block is stamped at a schema major this build cannot
    /// read: the document was written by a newer build, and its operations cannot
    /// be decoded into a score (Ruling B's preserved, never-materialized case).
    UnsupportedCanonicalMajor {
        /// The block's schema major.
        schema_major: u16,
    },
    /// The document carries a canonical base. Nothing this build writes has one
    /// (pruning is unimplemented), and a base cannot be reduced from an empty
    /// score, so it is refused by name.
    CanonicalBase,
    /// An envelope is in a historical layout this build recognizes and does not
    /// read: before the product's 1.0 release nothing is migrated, and a layout
    /// outside the supported boundary is refused by name.
    UnsupportedHistoricalEncoding {
        /// The value whose layout is historical.
        layout: &'static str,
    },
    /// An envelope failed to decode.
    Envelope(EnvelopeDecodeError),
    /// The document is read-only and grants no lease.
    ReadOnly(Vec<ReadOnlyReason>),
    /// The session holds no lease: it was opened on a bare score, or as a view.
    NotLeased,
    /// The session's lease came from another document instance.
    ForeignLease,
    /// The session's lease was revoked by a later lease of the same document, or
    /// reflects a committed generation the document has moved past.
    LeaseRevoked,
    /// A commit's outcome is unknown and [`EditorDocument::reconcile`] has not yet
    /// settled it; nothing is saved meanwhile.
    RecoveryRequired,
    /// The commit-point flush failed, so whether the commit landed is unknown. The
    /// session's units stay unsaved until [`EditorDocument::reconcile`] decides.
    Indeterminate(BundleError),
    /// The session over the document could not be built.
    Editor(EditorError),
}

impl std::fmt::Display for DocumentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DocumentError::Bundle(e) => write!(f, "{e}"),
            DocumentError::UnsupportedCanonicalMajor { schema_major } => write!(
                f,
                "an operation block is at schema major {schema_major}, which this build cannot \
                 read"
            ),
            DocumentError::CanonicalBase => {
                f.write_str("the document carries a canonical base, which this build cannot open")
            }
            DocumentError::UnsupportedHistoricalEncoding { layout } => write!(
                f,
                "an operation carries a {layout} in a historical layout this build does not read"
            ),
            DocumentError::Envelope(e) => write!(f, "an operation failed to decode: {e:?}"),
            DocumentError::ReadOnly(reasons) => write!(f, "the document is read-only: {reasons:?}"),
            DocumentError::NotLeased => f.write_str("the session was not leased from a document"),
            DocumentError::ForeignLease => {
                f.write_str("the session was leased from another document")
            }
            DocumentError::LeaseRevoked => f.write_str("the session's lease has been revoked"),
            DocumentError::RecoveryRequired => f.write_str(
                "an earlier save's outcome is unknown; reconcile the document before saving",
            ),
            DocumentError::Indeterminate(e) => write!(
                f,
                "the save's commit point failed ({e}); whether it landed is unknown until the \
                 document is reconciled"
            ),
            DocumentError::Editor(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for DocumentError {}

impl From<BundleError> for DocumentError {
    fn from(e: BundleError) -> Self {
        DocumentError::Bundle(e)
    }
}

impl From<EditorError> for DocumentError {
    fn from(e: EditorError) -> Self {
        DocumentError::Editor(e)
    }
}

/// A save that reached its commit point and made its units durable.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Saved {
    /// The bundle generation the save committed (unchanged when there was nothing
    /// to save).
    pub generation: u64,
    /// The envelopes promoted into the committed partition.
    pub envelopes: usize,
}

/// How [`EditorDocument::reconcile`] settled an indeterminate save.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Reconciled {
    /// The staged blocks are reachable from the selected generation and a fresh
    /// durability barrier succeeded: the units are saved and promoted.
    Saved(Saved),
    /// The selected generation does not reach the staged blocks: the units are
    /// unsaved and undoable, and the document is writable again.
    NotSaved,
    /// The fresh durability barrier failed: neither saved nor unsaved can be
    /// claimed, and the document stays in recovery.
    StillIndeterminate,
}

/// The committed partition reduced for the first lease, kept from the open so a
/// document is replayed once rather than twice before its first edit.
struct Prepared {
    replica: ReplicaId,
    base: Score,
    score: Score,
}

/// A document: a bundle, the operations committed to it, and at most one
/// writable session lease (module documentation).
pub struct EditorDocument<S: BlockStore> {
    bundle: Bundle<S>,
    committed: Committed,
    instance: u64,
    /// The number of the current lease; 0 before any is granted.
    lease_number: u64,
    read_only: Vec<ReadOnlyReason>,
    /// The chunk ids an indeterminate commit staged, until reconciled.
    unconfirmed: Option<Vec<ChunkId>>,
    prepared: Option<Prepared>,
}

impl<S: BlockStore> EditorDocument<S> {
    /// Creates a document in an empty `store` whose first generation commits
    /// `operations`: the genesis of a new score ([`ScoreSetup::operations`]), or an
    /// import's whole log. The operations are written as they are; a log that does
    /// not reduce cleanly yields a read-only document.
    pub fn create(store: S, operations: Vec<OperationEnvelope>) -> Result<Self, DocumentError> {
        let manifest = Manifest::empty(DocumentId(random_bytes()));
        let mut bundle =
            Bundle::create(store, FileUuid(random_bytes()), manifest, production_caps())?;
        if !operations.is_empty() {
            let staged = stage_blocks(&operations);
            let version = bundle.superblock().manifest_schema_version;
            bundle.commit_versioned(&staged, version, append_operation_roots)?;
        }
        Self::from_bundle(bundle, operations)
    }

    /// Opens the document in `store`: the bundle's cold open, then every envelope
    /// of every operation block decoded and replayed from an empty score.
    pub fn open(store: S) -> Result<Self, DocumentError> {
        let bundle = Bundle::open(store, production_caps())?;
        if let Some(schema_major) = bundle.anomalies().iter().find_map(|a| match a {
            IntegrityAnomaly::UnsupportedCanonicalChunkMajor { schema_major } => {
                Some(*schema_major)
            }
            _ => None,
        }) {
            return Err(DocumentError::UnsupportedCanonicalMajor { schema_major });
        }
        if bundle.manifest().canonical_base.is_some() {
            return Err(DocumentError::CanonicalBase);
        }
        let mut envelopes = Vec::new();
        for root in bundle.manifest().operation_roots.clone() {
            for bytes in bundle.read_operation_block(&root)? {
                envelopes.push(decode_envelope(&bytes).map_err(|e| match e {
                    EnvelopeDecodeError::UnsupportedLayout(layout) => {
                        DocumentError::UnsupportedHistoricalEncoding { layout }
                    }
                    other => DocumentError::Envelope(other),
                })?);
            }
        }
        Self::from_bundle(bundle, envelopes)
    }

    fn from_bundle(
        bundle: Bundle<S>,
        envelopes: Vec<OperationEnvelope>,
    ) -> Result<Self, DocumentError> {
        let committed = Committed::new(envelopes);
        let replica = ReplicaId::generate();
        let base = Score::empty(IdentityContext::new(replica));
        let materialized = committed.set().reduce_onto(&base);
        let mut read_only = Vec::new();
        if bundle.is_read_only() {
            read_only.push(ReadOnlyReason::Bundle {
                anomalies: bundle.anomalies().to_vec(),
            });
        }
        if !bundle.manifest().extension_declarations.is_empty() {
            read_only.push(ReadOnlyReason::Extensions);
        }
        if !materialized.state.is_clean() {
            read_only.push(ReadOnlyReason::Unclean {
                conflicts: materialized.state.conflicts.records().len(),
                pending: materialized.state.pending.len(),
                anomalies: materialized.state.anomalies.len(),
            });
        }
        Ok(EditorDocument {
            bundle,
            committed,
            instance: NEXT_INSTANCE.fetch_add(1, Ordering::Relaxed),
            lease_number: 0,
            read_only,
            unconfirmed: None,
            prepared: Some(Prepared {
                replica,
                base,
                score: materialized.score,
            }),
        })
    }

    /// Why the document is read-only; empty when it is writable.
    pub fn read_only_reasons(&self) -> &[ReadOnlyReason] {
        &self.read_only
    }

    /// Whether the document grants no lease.
    pub fn is_read_only(&self) -> bool {
        !self.read_only.is_empty()
    }

    /// The committed bundle generation.
    pub fn generation(&self) -> u64 {
        self.bundle.generation()
    }

    /// Every committed envelope, in the order the bundle's blocks hold them.
    pub fn committed_operations(&self) -> &[OperationEnvelope] {
        self.committed.envelopes()
    }

    /// The bundle, for its manifest and anomalies.
    pub fn bundle(&self) -> &Bundle<S> {
        &self.bundle
    }

    /// Closes the document, returning its store.
    pub fn into_store(self) -> S {
        self.bundle.into_store()
    }

    /// Grants the document's one writable lease: a session over the committed
    /// partition, minting under a fresh random replica (Ruling B: identity is
    /// fresh by default). A later lease revokes this one, whose saves are then
    /// refused. A read-only document, or one awaiting reconciliation, grants none.
    pub fn lease(
        &mut self,
        solver: Box<dyn ConstraintSolver>,
    ) -> Result<EditorSession, DocumentError> {
        if self.is_read_only() {
            return Err(DocumentError::ReadOnly(self.read_only.clone()));
        }
        if self.unconfirmed.is_some() {
            return Err(DocumentError::RecoveryRequired);
        }
        let Prepared {
            replica,
            base,
            score,
        } = self.take_prepared();
        self.lease_number += 1;
        let lease = Lease {
            instance: self.instance,
            number: self.lease_number,
            generation: self.generation(),
        };
        Ok(EditorSession::open_over(
            base,
            score,
            self.committed.clone(),
            solver,
            replica,
            Some(lease),
        )?)
    }

    /// A session over the committed partition that no save accepts: how a
    /// read-only document is drawn. Its edits apply locally and are never saved.
    pub fn view(&self, solver: Box<dyn ConstraintSolver>) -> Result<EditorSession, DocumentError> {
        let replica = ReplicaId::generate();
        let base = Score::empty(IdentityContext::new(replica));
        let score = self.committed.set().reduce_onto(&base).score;
        Ok(EditorSession::open_over(
            base,
            score,
            self.committed.clone(),
            solver,
            replica,
            None,
        )?)
    }

    /// The reduction the open prepared, or a fresh one under a fresh replica.
    fn take_prepared(&mut self) -> Prepared {
        if let Some(prepared) = self.prepared.take() {
            return prepared;
        }
        let replica = ReplicaId::generate();
        let base = Score::empty(IdentityContext::new(replica));
        let score = self.committed.set().reduce_onto(&base).score;
        Prepared {
            replica,
            base,
            score,
        }
    }

    /// Checks that `session` holds this document's current lease.
    fn check_lease(&self, session: &EditorSession) -> Result<Lease, DocumentError> {
        let lease = session.lease.ok_or(DocumentError::NotLeased)?;
        if lease.instance != self.instance {
            return Err(DocumentError::ForeignLease);
        }
        if lease.number != self.lease_number || lease.generation != self.generation() {
            return Err(DocumentError::LeaseRevoked);
        }
        Ok(lease)
    }

    /// Saves `session`'s applied units: commits them as new operation blocks and,
    /// once the commit point's flush has succeeded, promotes them into the committed
    /// partition, the document's and the session's alike. A successful save is an
    /// undo boundary; the redo stack survives it, since each undone unit's
    /// predecessors are the applied prefix the save committed.
    ///
    /// A commit that fails before its commit point returns
    /// [`DocumentError::Bundle`] with nothing promoted; one whose commit-point flush
    /// fails returns [`DocumentError::Indeterminate`], and the document then saves
    /// nothing until [`Self::reconcile`] settles it.
    pub fn save(&mut self, session: &mut EditorSession) -> Result<Saved, DocumentError> {
        // An unsettled commit is the document's state, whichever session asks.
        if self.unconfirmed.is_some() {
            return Err(DocumentError::RecoveryRequired);
        }
        let lease = self.check_lease(session)?;
        if self.is_read_only() {
            return Err(DocumentError::ReadOnly(self.read_only.clone()));
        }
        let units = session.applied.clone();
        if units.is_empty() {
            return Ok(Saved {
                generation: self.generation(),
                envelopes: 0,
            });
        }
        let staged = stage_blocks(&units);
        let version = self.bundle.superblock().manifest_schema_version;
        match self
            .bundle
            .commit_versioned(&staged, version, append_operation_roots)
        {
            Ok(()) => Ok(self.promote(session, lease, &units)),
            Err(e) if self.bundle.is_read_only() => {
                self.unconfirmed = Some(staged_ids(&staged));
                Err(DocumentError::Indeterminate(e))
            }
            Err(e) => Err(DocumentError::Bundle(e)),
        }
    }

    /// Moves `units` into the committed partition and the session's.
    fn promote(
        &mut self,
        session: &mut EditorSession,
        lease: Lease,
        units: &[OperationEnvelope],
    ) -> Saved {
        self.committed = self.committed.extended(units);
        self.prepared = None;
        session.promote(
            self.committed.clone(),
            Lease {
                generation: self.generation(),
                ..lease
            },
        );
        Saved {
            generation: self.generation(),
            envelopes: units.len(),
        }
    }

    /// Settles an indeterminate save (Ruling B's reconciliation): reopens the store
    /// and reads the selected generation, confirms whether every block the save
    /// staged is reachable from it, and only then runs a fresh durability barrier,
    /// which must succeed before the units are reported saved and promoted. A
    /// reopen that fails consumes the document, which is reopened from storage.
    pub fn reconcile(
        self,
        session: &mut EditorSession,
    ) -> Result<(Self, Reconciled), DocumentError> {
        let Some(staged) = self.unconfirmed else {
            return Ok((self, Reconciled::NotSaved));
        };
        let EditorDocument {
            bundle,
            committed,
            instance,
            lease_number,
            read_only,
            ..
        } = self;
        let bundle = Bundle::open(bundle.into_store(), production_caps())?;
        let reachable: std::collections::BTreeSet<ChunkId> = bundle
            .manifest()
            .operation_roots
            .iter()
            .map(|r| r.id)
            .collect();
        let landed = staged.iter().all(|id| reachable.contains(id));
        let mut document = EditorDocument {
            bundle,
            committed,
            instance,
            lease_number,
            read_only,
            unconfirmed: None,
            prepared: None,
        };
        if !landed {
            // The selected generation is the one before the save; the session keeps
            // its units, under a lease renewed for that generation.
            if let Some(lease) = session.lease.as_mut() {
                if lease.instance == instance && lease.number == lease_number {
                    lease.generation = document.generation();
                }
            }
            return Ok((document, Reconciled::NotSaved));
        }
        if document.bundle.sync().is_err() {
            document.unconfirmed = Some(staged);
            return Ok((document, Reconciled::StillIndeterminate));
        }
        let lease = match session.lease {
            Some(lease) if lease.instance == instance && lease.number == lease_number => lease,
            Some(_) => return Err(DocumentError::LeaseRevoked),
            None => return Err(DocumentError::NotLeased),
        };
        let units = session.applied.clone();
        let saved = document.promote(session, lease, &units);
        Ok((document, Reconciled::Saved(saved)))
    }
}

/// The manifest a save commits: the previous one with the staged blocks added to
/// its operation roots. The non-canonical accelerators that describe the whole
/// operation set (the operation index, acceleration snapshots, the text projection
/// and the integrity index) would no longer describe it, so they are dropped; a
/// block's own summary still describes its block and is kept.
fn append_operation_roots(ctx: &CommitContext) -> Manifest {
    let mut manifest = ctx.previous_manifest.clone();
    manifest
        .operation_roots
        .extend(ctx.new_chunks.iter().copied());
    manifest.operation_index_root = None;
    manifest.acceleration_snapshots.clear();
    manifest.text_projection_root = None;
    manifest.integrity_root = None;
    manifest
}

/// `envelopes` packed into operation blocks at the bundle's soft size target, each
/// stamped with the highest schema major among its own envelopes and the epoch of
/// any later-added discriminant they carry (minimal stamping: a block is never
/// stamped above what its content needs, nor below).
fn stage_blocks(envelopes: &[OperationEnvelope]) -> Vec<StagedChunk> {
    let payloads: Vec<Vec<u8>> = envelopes
        .iter()
        .map(CanonicalEncode::to_canonical_bytes)
        .collect();
    let mut staged = Vec::new();
    let mut next = 0;
    for block in pack_operation_blocks(&payloads) {
        let count = decode_block(&block)
            .expect("a block this function just packed decodes")
            .len();
        let members = &envelopes[next..next + count];
        next += count;
        let major = members
            .iter()
            .map(OperationEnvelope::schema_major)
            .max()
            .unwrap_or(0);
        let epoch = operation_block_introduced_minor(members);
        staged.push(StagedChunk::operation_block_versioned(
            block,
            SchemaVersion::for_major_at_epoch(major, epoch),
        ));
    }
    staged
}

/// The content ids of staged chunks, as the manifest's roots will name them.
fn staged_ids(staged: &[StagedChunk]) -> Vec<ChunkId> {
    staged
        .iter()
        .map(|chunk| {
            chunk_id(
                ChunkKind::OperationEnvelopeBlock,
                chunk.schema_version,
                &chunk.payload,
            )
        })
        .collect()
}

/// Sixteen bytes from the platform's entropy source, for a new document's and
/// file's identities.
fn random_bytes() -> [u8; 16] {
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).expect("platform CSPRNG unavailable");
    bytes
}

/// One staff of a new score.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct StaffSetup {
    /// The instrument's name, shown at the system's start.
    pub name: String,
    /// The staff's clef.
    pub clef: Clef,
}

/// A new score's setup, as a new-score dialog collects it: its title, staves,
/// meter, key and length. [`Self::operations`] turns it into the operations that
/// build it from an empty score, which [`EditorDocument::create`] commits as the
/// document's first generation.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ScoreSetup {
    /// The title, if any.
    pub title: Option<String>,
    /// The staves, top to bottom, one instrument each.
    pub staves: Vec<StaffSetup>,
    /// The meter's numerator.
    pub beats: u16,
    /// The meter's denominator, a power of two.
    pub beat_unit: u16,
    /// The key, in fifths from C (negative for flats).
    pub fifths: i8,
    /// The number of measures.
    pub measures: u32,
}

impl ScoreSetup {
    /// One treble staff in 4/4, C major, with `measures` empty measures.
    pub fn single_staff(name: &str, measures: u32) -> Self {
        ScoreSetup {
            title: None,
            staves: vec![StaffSetup {
                name: name.to_owned(),
                clef: Clef::treble(),
            }],
            beats: 4,
            beat_unit: 4,
            fifths: 0,
            measures,
        }
    }

    /// The operations that build this score from an empty one, authored under
    /// `replica` as one causal chain: per staff an instrument and a staff, then one
    /// metric region holding the whole score, its meter, a staff instance with one
    /// primary voice per staff, and the measures on every staff. `None` when the
    /// meter or key cannot be represented (a denominator that is not a power of
    /// two, or a key beyond seven accidentals).
    pub fn operations(&self, replica: ReplicaId) -> Option<Vec<OperationEnvelope>> {
        let mut identity = IdentityContext::new(replica);
        let mut ops: Vec<OperationKind> = Vec::new();
        if let Some(title) = &self.title {
            ops.push(OperationKind::SetMetadata(SetMetadataOp {
                metadata: ScoreMetadata {
                    title: Some(title.clone()),
                    composer: None,
                    copyright: None,
                    subtitle: None,
                    lyricist: None,
                    arranger: None,
                    creation_timestamp: Timestamp(0),
                    modification_timestamp: Timestamp(0),
                    additional: Vec::new(),
                },
            }));
        }
        let key = KeySignature::new(self.fifths)?;
        let denominator = PowerOfTwo::new(self.beat_unit)?;
        let unit = RationalTime::new(1, i64::from(self.beat_unit))?;
        let measure_length = RationalTime::new(i64::from(self.beats), i64::from(self.beat_unit))?;

        let mut staves = Vec::with_capacity(self.staves.len());
        for staff in &self.staves {
            let instrument_id = identity.mint();
            let mut instrument = Instrument::new(instrument_id, staff.name.clone());
            instrument.default_clef = staff.clef;
            ops.push(OperationKind::CreateInstrument(CreateInstrumentOp {
                instrument,
            }));
            let staff_id = identity.mint();
            ops.push(OperationKind::CreateStaff(CreateStaffOp {
                staff: Staff {
                    id: staff_id,
                    name: staff.name.clone(),
                    abbreviation: None,
                    instrument: instrument_id,
                    default_staff_lines: StaffLineConfiguration::default(),
                    group: None,
                    default_clef: staff.clef,
                },
            }));
            staves.push((staff_id, staff.clef));
        }

        let region = identity.mint();
        ops.push(OperationKind::CreateRegion(CreateRegionOp {
            region: Region {
                id: region,
                time_model: RegionTimeModel::Metric(MetricTimeModel::default()),
                content: RegionContent::StaffBased(Default::default()),
                time_extent: TimeExtent {
                    start: TimeAnchor::WallClock {
                        time: WallClockTime(0),
                    },
                    end: TimeAnchor::WallClock {
                        time: WallClockTime(1),
                    },
                },
                staff_extent: StaffExtent { staves: Vec::new() },
                local_tempo_map: None,
                permits_spanning_slurs: false,
            },
        }));
        let signature_id = identity.mint();
        let beat_groups = (0..self.beats)
            .map(|i| epiphany_core::BeatGroup {
                duration: MusicalDuration(unit.clone()),
                subdivision: Some(MusicalDuration(unit.clone())),
                accent: u8::from(i == 0),
            })
            .collect();
        let signature = TimeSignature::new(
            signature_id,
            TimeSignatureDisplay::Standard {
                numerator: self.beats,
                denominator,
            },
            MusicalDuration(measure_length.clone()),
            beat_groups,
        )?;
        ops.push(OperationKind::SetTimeSignature(SetTimeSignatureOp {
            region,
            anchor: region_offset(region, RationalTime::zero()),
            time_signature: Some(signature),
        }));

        for (staff_id, clef) in &staves {
            let instance_id = identity.mint();
            let mut instance = StaffInstance::new(instance_id, *staff_id);
            instance.clef_sequence = vec![epiphany_core::ClefChange {
                anchor: region_offset(region, RationalTime::zero()),
                clef: *clef,
            }];
            if self.fifths != 0 {
                instance.key_sequence = vec![KeySignatureChange {
                    anchor: region_offset(region, RationalTime::zero()),
                    key,
                }];
            }
            ops.push(OperationKind::CreateStaffInstance(CreateStaffInstanceOp {
                region,
                instance,
            }));
            ops.push(OperationKind::CreateVoice(CreateVoiceOp {
                staff_instance: instance_id,
                voice: Voice {
                    id: identity.mint(),
                    events: Vec::new(),
                    default_stem_direction: None,
                    is_primary: true,
                    origin: VoiceOrigin::UserDeclared,
                },
            }));
            for m in 0..self.measures {
                let onset = measure_length.mul(&RationalTime::from_int(i32::try_from(m).ok()?));
                ops.push(OperationKind::CreateMeasure(CreateMeasureOp {
                    instance: instance_id,
                    measure: Measure {
                        id: identity.mint(),
                        start: region_offset(region, onset),
                        time_signature: (m == 0).then_some(signature_id),
                        explicit_number: None,
                        number_visibility: MeasureNumberVisibility::Auto,
                    },
                }));
            }
        }
        Some(chain(replica, ops))
    }
}

/// A position `offset` whole notes after `region`'s start.
fn region_offset(region: epiphany_core::RegionId, offset: RationalTime) -> TimeAnchor {
    TimeAnchor::Region {
        id: region,
        edge: RegionEdge::Start,
        offset: epiphany_core::AnchorOffset::Musical(MusicalDuration(offset)),
    }
}

/// `ops` as one author's causal chain under `replica`, each envelope covering the
/// one before it.
fn chain(replica: ReplicaId, ops: Vec<OperationKind>) -> Vec<OperationEnvelope> {
    ops.into_iter()
        .enumerate()
        .map(|(counter, op)| {
            let counter = counter as u64;
            let id = OperationId::new(replica, counter);
            OperationEnvelope {
                id,
                author: AuthorId(0),
                stamp: OperationStamp::new(
                    HybridLogicalClock::new(WallClockTime(counter as i64), 0),
                    id,
                ),
                causal_context: match counter {
                    0 => CausalContext::new(),
                    n => CausalContext::new().with_seen(replica, n - 1),
                },
                transaction: None,
                payload: OperationPayload::Primitive(op),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ruling B's required frontier-builder case: a counter gap leaves the floor at
    /// the contiguous prefix and the operation past it a dot, never floor `r:2`.
    #[test]
    fn a_counter_gap_becomes_a_dot_not_a_floor() {
        let r = ReplicaId(9);
        let frontier = frontier_of([OperationId::new(r, 0), OperationId::new(r, 2)]);
        assert_eq!(frontier.vector.get(&r), Some(&0));
        assert_eq!(
            frontier.dots.iter().copied().collect::<Vec<_>>(),
            vec![OperationId::new(r, 2)]
        );
        assert!(!frontier.covers(OperationId::new(r, 1)));
    }

    /// A replica whose history does not start at counter 0 contributes only dots.
    #[test]
    fn a_replica_without_counter_zero_has_no_floor() {
        let r = ReplicaId(9);
        let s = ReplicaId(10);
        let frontier = frontier_of([
            OperationId::new(r, 3),
            OperationId::new(r, 4),
            OperationId::new(s, 0),
            OperationId::new(s, 1),
        ]);
        assert_eq!(frontier.vector.get(&r), None);
        assert_eq!(frontier.vector.get(&s), Some(&1));
        assert_eq!(frontier.dots.len(), 2);
    }
}
