//! Conflict records and the content-derived [`ConflictId`] (Chapter 6
//! §"Conflict Records").
//!
//! When an operation cannot apply cleanly, the reduction produces a
//! `Conflicted` effect referencing a [`ConflictRecord`] in the score's conflict
//! registry. Conflict records are first-class, *canonical* graph objects:
//! stable, addressable, visible to users, and resolvable by a later
//! `ResolveConflict` operation.
//!
//! Because conflict records are produced during deterministic reduction rather
//! than authored by replicas, their identifiers MUST be **content-derived**:
//! two replicas reducing the same operation set produce the same conflicts with
//! the same ids ([`derive_conflict_id`]). An ordinal or local counter in the
//! preimage is forbidden — conflicts that share kind, causing operations, and
//! affected objects are by definition the same conflict (Chapter 6 §6.4.3).
//!
//! Integrity *anomalies* ([`crate::IntegrityAnomaly`]) are deliberately a
//! separate type, not a `ConflictKind`: a conflict is an ordinary
//! canonical-state fact (two valid edits collide); an anomaly is a structural
//! failure that takes the document out of ordinary canonical operation
//! (Chapter 6 / Chapter 5 §"System-Derived Counter Collisions"; Pass 10).

use epiphany_core::{OperationId, RegionId, TransactionId, TypedObjectId};
use epiphany_determinism::{
    sorted_canonical, CanonicalByteOrder, CanonicalEncode, DomainTag, Preimage,
};

use crate::encode::{push_canon, push_seq, push_str, push_tag};
use crate::support::{ConflictKindRegistryId, ResolutionRegistryId};

/// A content-derived conflict identifier: `trunc128(BLAKE3("MUSCCONF" || …))`
/// (Chapter 6 §6.4.3). 128-bit, canonical 16-byte big-endian form, ordered
/// byte-lexicographically — the order the conflict registry iterates in
/// (Appendix D §"Ordered Iteration": "Conflict records: ascending by
/// ConflictId").
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Debug)]
pub struct ConflictId(pub u128);

impl ConflictId {
    /// The canonical 16-byte big-endian form.
    #[inline]
    pub const fn canonical_bytes(self) -> [u8; 16] {
        self.0.to_be_bytes()
    }
}

impl CanonicalEncode for ConflictId {
    #[inline]
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.canonical_bytes());
    }
}
impl CanonicalByteOrder for ConflictId {}

/// A path to a specific field, naming what two concurrent operations collided
/// on in a [`ConflictKind::StructuralFieldCollision`]. Canonical text
/// (Appendix D §"Text and Unicode").
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct FieldPath(pub String);

impl CanonicalEncode for FieldPath {
    #[inline]
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_str(out, &self.0);
    }
}

/// The kind of conflict, governing the resolution options (Chapter 6 §6.4.1).
///
/// The canonical byte form (via [`CanonicalEncode`]) includes the kind
/// discriminant and **all** payload, so distinct conflicts have distinct
/// preimages by construction (Chapter 6 §6.4.3 requirement).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ConflictKind {
    /// Two concurrent operations wrote the same non-LWW field. The winner's
    /// effect is materialized; the loser's is preserved here for inspection.
    StructuralFieldCollision {
        winner: OperationId,
        loser: OperationId,
        field: FieldPath,
    },
    /// A transaction's members were partially applicable; the whole
    /// transaction was rejected.
    TransactionConflict {
        transaction: TransactionId,
        failed_members: Vec<OperationId>,
    },
    /// An operation targeted a tombstoned object the kind's rules could not
    /// repair.
    TombstonedTarget {
        target: TypedObjectId,
        operation: OperationId,
    },
    /// Re-anchoring failed: no deterministic target could be found.
    ReanchorFailure {
        original_referent: TypedObjectId,
        referencing_object: TypedObjectId,
    },
    /// A region time-model migration produced contained events whose
    /// coordinate kinds are incompatible with the new model.
    TimeModelMigrationFailure {
        region: RegionId,
        incompatible_events: Vec<TypedObjectId>,
    },
    /// A registered extension operation's reduction failed; opaque to the core.
    ExtensionConflict {
        kind_id: ConflictKindRegistryId,
        details: Vec<u8>,
    },
}

impl ConflictKind {
    /// The discriminant byte; part of the canonical preimage.
    fn discriminant(&self) -> u8 {
        match self {
            ConflictKind::StructuralFieldCollision { .. } => 0,
            ConflictKind::TransactionConflict { .. } => 1,
            ConflictKind::TombstonedTarget { .. } => 2,
            ConflictKind::ReanchorFailure { .. } => 3,
            ConflictKind::TimeModelMigrationFailure { .. } => 4,
            ConflictKind::ExtensionConflict { .. } => 5,
        }
    }

    /// The canonical byte form of the kind, as required by
    /// [`derive_conflict_id`] (Chapter 6 §6.4.3).
    pub fn canonical_bytes(&self) -> Vec<u8> {
        self.to_canonical_bytes()
    }
}

impl CanonicalEncode for ConflictKind {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_tag(out, self.discriminant());
        match self {
            ConflictKind::StructuralFieldCollision {
                winner,
                loser,
                field,
            } => {
                push_canon(out, winner);
                push_canon(out, loser);
                push_canon(out, field);
            }
            ConflictKind::TransactionConflict {
                transaction,
                failed_members,
            } => {
                push_canon(out, transaction);
                // Failed members in canonical order so the preimage is stable.
                push_seq(out, &sorted_canonical(failed_members.clone()));
            }
            ConflictKind::TombstonedTarget { target, operation } => {
                push_canon(out, target);
                push_canon(out, operation);
            }
            ConflictKind::ReanchorFailure {
                original_referent,
                referencing_object,
            } => {
                push_canon(out, original_referent);
                push_canon(out, referencing_object);
            }
            ConflictKind::TimeModelMigrationFailure {
                region,
                incompatible_events,
            } => {
                push_canon(out, region);
                push_seq(out, &sorted_canonical(incompatible_events.clone()));
            }
            ConflictKind::ExtensionConflict { kind_id, details } => {
                push_canon(out, kind_id);
                crate::encode::push_lp_bytes(out, details);
            }
        }
    }
}

/// How a conflict was resolved (Chapter 6 §6.4.1).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ResolutionAction {
    /// Accept the losing operation's effect (replacing the winner).
    AcceptLoser,
    /// Reapply the winner explicitly (clears the conflict, no semantic change).
    KeepWinner,
    /// A user-authored replacement that supersedes both.
    Override { override_operation: OperationId },
    /// Re-anchor to a user-chosen target.
    Reanchor { new_target: TypedObjectId },
    /// Dismiss the conflict without changing the materialized graph: the user
    /// acknowledges it and accepts the current (winner) state. Selects the
    /// `Dismissed` resolution state (Pass 11, item 2.5; Chapter 6).
    Dismiss,
    /// Custom resolution for a registered conflict kind.
    Registered(ResolutionRegistryId),
}

impl ResolutionAction {
    fn discriminant(&self) -> u8 {
        match self {
            ResolutionAction::AcceptLoser => 0,
            ResolutionAction::KeepWinner => 1,
            ResolutionAction::Override { .. } => 2,
            ResolutionAction::Reanchor { .. } => 3,
            ResolutionAction::Dismiss => 4,
            ResolutionAction::Registered(_) => 5,
        }
    }
}

impl CanonicalEncode for ResolutionAction {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_tag(out, self.discriminant());
        match self {
            ResolutionAction::AcceptLoser
            | ResolutionAction::KeepWinner
            | ResolutionAction::Dismiss => {}
            ResolutionAction::Override { override_operation } => {
                push_canon(out, override_operation)
            }
            ResolutionAction::Reanchor { new_target } => push_canon(out, new_target),
            ResolutionAction::Registered(id) => push_canon(out, id),
        }
    }
}

/// The current resolution state of a conflict (Chapter 6 §6.4.1). Conflicts
/// begin `Unresolved`; a later `ResolveConflict` operation transitions them.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ConflictResolutionState {
    Unresolved,
    Resolved {
        by: OperationId,
        action: ResolutionAction,
    },
    Dismissed {
        by: OperationId,
    },
}

impl ConflictResolutionState {
    fn discriminant(&self) -> u8 {
        match self {
            ConflictResolutionState::Unresolved => 0,
            ConflictResolutionState::Resolved { .. } => 1,
            ConflictResolutionState::Dismissed { .. } => 2,
        }
    }
}

impl CanonicalEncode for ConflictResolutionState {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_tag(out, self.discriminant());
        match self {
            ConflictResolutionState::Unresolved => {}
            ConflictResolutionState::Resolved { by, action } => {
                push_canon(out, by);
                push_canon(out, action);
            }
            ConflictResolutionState::Dismissed { by } => push_canon(out, by),
        }
    }
}

/// A first-class conflict record (Chapter 6 §6.4.1).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ConflictRecord {
    /// Content-derived identifier ([`derive_conflict_id`]).
    pub id: ConflictId,
    /// The operations that participated. At least two for a true conflict; one
    /// for an operation that failed precondition checking under reduction.
    pub caused_by: Vec<OperationId>,
    /// The kind of conflict.
    pub kind: ConflictKind,
    /// Objects affected, for diagnostic and UI navigation.
    pub affected_objects: Vec<TypedObjectId>,
    /// Current resolution state.
    pub resolution_state: ConflictResolutionState,
}

impl ConflictRecord {
    /// Builds an unresolved conflict record, deriving its content id from the
    /// kind, causing operations, and affected objects (Chapter 6 §6.4.3). The
    /// `caused_by` and `affected_objects` vectors are stored in canonical order
    /// so the record's own canonical bytes are stable regardless of how the
    /// reduction assembled them.
    pub fn new(
        kind: ConflictKind,
        caused_by: Vec<OperationId>,
        affected_objects: Vec<TypedObjectId>,
    ) -> Self {
        let id = derive_conflict_id(&kind, &caused_by, &affected_objects);
        ConflictRecord {
            id,
            caused_by: sorted_canonical(caused_by),
            kind,
            affected_objects: sorted_canonical(affected_objects),
            resolution_state: ConflictResolutionState::Unresolved,
        }
    }
}

impl CanonicalEncode for ConflictRecord {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        push_canon(out, &self.id);
        push_seq(out, &self.caused_by);
        push_canon(out, &self.kind);
        push_seq(out, &self.affected_objects);
        push_canon(out, &self.resolution_state);
    }
}

/// Derives a [`ConflictId`] from a conflict's content (Chapter 6 §6.4.3).
///
/// The preimage is `"MUSCCONF" || kind.canonical_bytes() || sorted_ops ||
/// sorted_objs`, where causing operations and affected objects are each sorted
/// lexicographically by canonical bytes. No ordinal or local counter enters the
/// preimage: conflicts sharing kind, causing operations, and affected objects
/// are by definition the same conflict.
pub fn derive_conflict_id(
    kind: &ConflictKind,
    causing_operations: &[OperationId],
    affected_objects: &[TypedObjectId],
) -> ConflictId {
    let mut p = Preimage::new(DomainTag::CONFLICT);
    p.push_bytes(&kind.canonical_bytes());
    for op in sorted_canonical(causing_operations.to_vec()) {
        p.push_bytes(&op.canonical_bytes());
    }
    for obj in sorted_canonical(affected_objects.to_vec()) {
        p.push_bytes(&obj.canonical_bytes());
    }
    ConflictId(p.finish_trunc128())
}

/// The score's conflict registry (Chapter 6 §6.4.2): part of canonical
/// materialized state. Records are kept in the normative order — ascending by
/// [`ConflictId`] — so the registry's canonical bytes never depend on the order
/// in which the reduction discovered the conflicts.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct ConflictRegistry {
    records: Vec<ConflictRecord>,
}

impl ConflictRegistry {
    /// An empty registry.
    #[inline]
    pub fn new() -> Self {
        ConflictRegistry::default()
    }

    /// Inserts a record, keeping the registry ordered by `ConflictId`. If a
    /// record with the same content id already exists (the same conflict
    /// re-derived), the insert is idempotent — content-derived identity means
    /// re-discovering a conflict is not a second conflict.
    pub fn insert(&mut self, record: ConflictRecord) {
        match self.records.binary_search_by(|r| r.id.cmp(&record.id)) {
            Ok(_) => {} // already present; content-derived identity ⇒ idempotent
            Err(pos) => self.records.insert(pos, record),
        }
    }

    /// The records, in canonical (ascending-`ConflictId`) order.
    #[inline]
    pub fn records(&self) -> &[ConflictRecord] {
        &self.records
    }

    /// A mutable handle to the record with `id`, if present.
    pub fn get_mut(&mut self, id: ConflictId) -> Option<&mut ConflictRecord> {
        self.records
            .binary_search_by(|r| r.id.cmp(&id))
            .ok()
            .map(|pos| &mut self.records[pos])
    }

    /// Whether the registry holds any records.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

impl CanonicalEncode for ConflictRegistry {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        // Already in ConflictId order by construction.
        push_seq(out, &self.records);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use epiphany_core::{EventId, ReplicaId};

    fn op(r: u64, c: u64) -> OperationId {
        OperationId::new(ReplicaId(r), c)
    }
    fn obj(n: u128) -> TypedObjectId {
        TypedObjectId::Event(EventId::from_raw(n))
    }

    #[test]
    fn conflict_id_is_order_independent_in_its_inputs() {
        let kind = ConflictKind::TombstonedTarget {
            target: obj(7),
            operation: op(1, 1),
        };
        let a = derive_conflict_id(&kind, &[op(1, 1), op(2, 2)], &[obj(7), obj(3)]);
        let b = derive_conflict_id(&kind, &[op(2, 2), op(1, 1)], &[obj(3), obj(7)]);
        assert_eq!(a, b, "causing-op / affected-object order must not matter");
    }

    #[test]
    fn distinct_kinds_yield_distinct_ids() {
        let k1 = ConflictKind::TombstonedTarget {
            target: obj(7),
            operation: op(1, 1),
        };
        let k2 = ConflictKind::ReanchorFailure {
            original_referent: obj(7),
            referencing_object: obj(1),
        };
        assert_ne!(
            derive_conflict_id(&k1, &[op(1, 1)], &[obj(7)]),
            derive_conflict_id(&k2, &[op(1, 1)], &[obj(7)]),
        );
    }

    #[test]
    fn registry_is_sorted_and_idempotent_on_same_content() {
        let mut reg = ConflictRegistry::new();
        let mk = |t: u128| {
            ConflictRecord::new(
                ConflictKind::TombstonedTarget {
                    target: obj(t),
                    operation: op(1, 1),
                },
                vec![op(1, 1)],
                vec![obj(t)],
            )
        };
        let a = mk(5);
        reg.insert(mk(9));
        reg.insert(a.clone());
        reg.insert(mk(1));
        reg.insert(a.clone()); // re-discovering the same conflict
        let ids: Vec<_> = reg.records().iter().map(|r| r.id).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted, "registry must be ConflictId-ordered");
        // a inserted twice but present once.
        assert_eq!(reg.records().iter().filter(|r| r.id == a.id).count(), 1);
    }
}
