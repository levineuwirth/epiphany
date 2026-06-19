//! Edit barriers (Chapter 8 §"Forward Compatibility and Edit Barriers").
//!
//! An edit barrier protects an extension's invariants: it prohibits a *class* of
//! operations over a scope, so it references the discriminator-only
//! [`OperationKindTag`] (Agent C's canonical type — Chapter 6) in
//! `prohibited_operation_kinds`, never a concrete payload (Chapter 8: "A barrier
//! prohibits an operation class, not one exact payload"). The QUICKSTART assigns
//! the barrier *types* to Agent E, keyed on `OperationKindTag`.
//!
//! An edit matching a barrier's scope, affected object kinds, prohibited
//! operation kinds, and condition is prohibited unless the user performs an
//! explicit unsafe edit (Chapter 8 §"Behavior Under Unknown Extensions"). v0
//! models the barrier *types* and a conservative `prohibits_edit` predicate; the
//! unsafe-edit mechanism and registry evaluation are bundle/extension concerns.
//!
//! Barriers are stored in the bundle in the spec, so the types here carry a
//! canonical encoding (Appendix D): set-valued fields are emitted in canonical
//! byte order, so two barriers with the same sets in different orders encode
//! identically.

use epiphany_core::{AnalysisLayerId, PitchSpaceId, RegionId, StaffInstanceId, TypedObjectId};
use epiphany_determinism::CanonicalEncode;
use epiphany_ops::OperationKindTag;

/// The kind of a score-graph object a barrier protects (Chapter 8:
/// `ObjectKind`). v0 represents it by the `TypedObjectId` discriminant — the
/// kind of object, independent of which one — which is the natural object-class
/// key in this codebase.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ObjectKind(pub u16);

impl ObjectKind {
    /// The object kind of a concrete object.
    pub fn of(object: &TypedObjectId) -> ObjectKind {
        ObjectKind(object.discriminant())
    }
}

impl CanonicalEncode for ObjectKind {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.0.to_le_bytes());
    }
}

/// Registry id for an extension-defined [`BarrierScope::Registered`].
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct BarrierScopeRegistryId(pub u128);

/// Registry id for an extension-defined [`BarrierCondition::Registered`].
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct BarrierConditionRegistryId(pub u128);

/// A reference to the declaring extension of a barrier condition (the spec's
/// `ExtensionId`). Opaque in v0 — the extension subsystem lives in the bundle.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ExtensionRef(pub u128);

/// The scope of an edit barrier (Chapter 8: `BarrierScope`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum BarrierScope {
    WholeScore,
    Region(RegionId),
    StaffInstance(StaffInstanceId),
    AnalysisLayer(AnalysisLayerId),
    ObjectSet(Vec<TypedObjectId>),
    PitchSpace(PitchSpaceId),
    TuningContext,
    Registered(BarrierScopeRegistryId),
}

/// A narrowing condition for an edit barrier (Chapter 8: `BarrierCondition`).
/// The barrier applies only when its scope, object kinds, operation kinds, and
/// this condition all match.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum BarrierCondition {
    /// Applies unconditionally within its scope.
    Always,
    /// Applies only while the named object exists (not tombstoned).
    ObjectExists(TypedObjectId),
    /// Applies only when the named object carries data from the named extension.
    ObjectHasExtensionData {
        object: TypedObjectId,
        extension: ExtensionRef,
    },
    /// Conjunction: all conditions must match.
    All(Vec<BarrierCondition>),
    /// Disjunction: any condition matching is sufficient.
    Any(Vec<BarrierCondition>),
    /// Negation: the inner condition must not match.
    Not(Box<BarrierCondition>),
    /// Extension-evaluated condition. Core implementations treat it as `Always`
    /// (conservative — Chapter 8).
    Registered(BarrierConditionRegistryId),
}

/// Answers the score-state questions a barrier condition asks: whether an object
/// is live (not tombstoned) and whether it carries a given extension's data. The
/// editor — which holds the score — implements this so *known* conditions are
/// evaluated precisely.
pub trait EditOracle {
    /// Whether `object` currently exists (is not tombstoned).
    fn object_exists(&self, object: &TypedObjectId) -> bool;
    /// Whether `object` carries data declared by `extension`.
    fn has_extension_data(&self, object: &TypedObjectId, extension: ExtensionRef) -> bool;
}

/// A trivial oracle: every object exists and carries no extension data. Useful
/// as a default when extension data is irrelevant.
pub struct AlwaysLiveOracle;

impl EditOracle for AlwaysLiveOracle {
    fn object_exists(&self, _object: &TypedObjectId) -> bool {
        true
    }
    fn has_extension_data(&self, _object: &TypedObjectId, _extension: ExtensionRef) -> bool {
        false
    }
}

impl BarrierCondition {
    /// Evaluates the condition against `oracle`. **Known** leaf conditions
    /// (`ObjectExists`, `ObjectHasExtensionData`) are evaluated *precisely* via
    /// the oracle; only an unknown `Registered` condition is treated
    /// conservatively (as active), so a core implementation never silently drops
    /// a barrier it cannot evaluate (Chapter 8 §"Behavior Under Unknown
    /// Extensions"). The boolean combinators are applied literally.
    pub fn is_active(&self, oracle: &dyn EditOracle) -> bool {
        !matches!(self.evaluate(oracle), ConditionEvaluation::Inactive)
    }

    fn evaluate(&self, oracle: &dyn EditOracle) -> ConditionEvaluation {
        match self {
            BarrierCondition::Always => ConditionEvaluation::Active,
            BarrierCondition::ObjectExists(o) => {
                ConditionEvaluation::from_bool(oracle.object_exists(o))
            }
            BarrierCondition::ObjectHasExtensionData { object, extension } => {
                ConditionEvaluation::from_bool(oracle.has_extension_data(object, *extension))
            }
            BarrierCondition::All(cs) => {
                let mut unknown = false;
                for condition in cs {
                    match condition.evaluate(oracle) {
                        ConditionEvaluation::Inactive => return ConditionEvaluation::Inactive,
                        ConditionEvaluation::Unknown => unknown = true,
                        ConditionEvaluation::Active => {}
                    }
                }
                if unknown {
                    ConditionEvaluation::Unknown
                } else {
                    ConditionEvaluation::Active
                }
            }
            BarrierCondition::Any(cs) => {
                let mut unknown = false;
                for condition in cs {
                    match condition.evaluate(oracle) {
                        ConditionEvaluation::Active => return ConditionEvaluation::Active,
                        ConditionEvaluation::Unknown => unknown = true,
                        ConditionEvaluation::Inactive => {}
                    }
                }
                if unknown {
                    ConditionEvaluation::Unknown
                } else {
                    ConditionEvaluation::Inactive
                }
            }
            BarrierCondition::Not(condition) => match condition.evaluate(oracle) {
                ConditionEvaluation::Active => ConditionEvaluation::Inactive,
                ConditionEvaluation::Inactive => ConditionEvaluation::Active,
                ConditionEvaluation::Unknown => ConditionEvaluation::Unknown,
            },
            BarrierCondition::Registered(_) => ConditionEvaluation::Unknown,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum ConditionEvaluation {
    Active,
    Inactive,
    Unknown,
}

impl ConditionEvaluation {
    fn from_bool(value: bool) -> Self {
        if value {
            ConditionEvaluation::Active
        } else {
            ConditionEvaluation::Inactive
        }
    }
}

/// An edit barrier (Chapter 8: `EditBarrier`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct EditBarrier {
    pub scope: BarrierScope,
    /// Object kinds protected by this barrier.
    pub affected_object_kinds: Vec<ObjectKind>,
    /// Operation classes this barrier prohibits (Chapter 8: keyed on
    /// `OperationKindTag`, not `OperationKind`).
    pub prohibited_operation_kinds: Vec<OperationKindTag>,
    /// Additional narrowing condition.
    pub condition: BarrierCondition,
}

/// The structural location of a candidate edit's object, supplied by the
/// editor (which has the score) so that *known* barrier scopes
/// (`Region`/`StaffInstance`/`AnalysisLayer`/`PitchSpace`) are evaluated
/// **precisely** rather than conservatively. Each field is the object's
/// containing entity, or `None` if it has none of that kind.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct EditContext {
    pub region: Option<RegionId>,
    pub staff_instance: Option<StaffInstanceId>,
    pub analysis_layer: Option<AnalysisLayerId>,
    pub pitch_space: Option<PitchSpaceId>,
}

impl EditBarrier {
    /// Whether this barrier prohibits applying an operation of class `op` to
    /// `object`, given the object's structural location `ctx`.
    ///
    /// Known scopes are evaluated **precisely** against `ctx`: a `Region`
    /// barrier prohibits only objects in that region, etc. Only genuinely
    /// unknown narrowing — a `Registered` scope, an unknown `Registered`
    /// condition — is treated conservatively (as matching), so a core
    /// implementation never silently drops a barrier it cannot fully evaluate
    /// (Chapter 8 §"Behavior Under Unknown Extensions"). An empty
    /// `affected_object_kinds` matches any kind.
    pub fn prohibits_edit(
        &self,
        op: OperationKindTag,
        object: &TypedObjectId,
        ctx: &EditContext,
        oracle: &dyn EditOracle,
    ) -> bool {
        self.prohibited_operation_kinds.contains(&op)
            && (self.affected_object_kinds.is_empty()
                || self.affected_object_kinds.contains(&ObjectKind::of(object)))
            && self.scope_admits(object, ctx)
            && self.condition.is_active(oracle)
    }

    /// Whether the barrier's scope admits `object`. `WholeScore`/`TuningContext`
    /// are score-wide; `ObjectSet` and the structural scopes are checked
    /// precisely against `ctx`; only a `Registered` (unknown-extension) scope is
    /// treated conservatively as admitting.
    fn scope_admits(&self, object: &TypedObjectId, ctx: &EditContext) -> bool {
        match &self.scope {
            BarrierScope::WholeScore | BarrierScope::TuningContext => true,
            BarrierScope::ObjectSet(objs) => objs.contains(object),
            BarrierScope::Region(r) => ctx.region.as_ref() == Some(r),
            BarrierScope::StaffInstance(si) => ctx.staff_instance.as_ref() == Some(si),
            BarrierScope::AnalysisLayer(a) => ctx.analysis_layer.as_ref() == Some(a),
            BarrierScope::PitchSpace(ps) => ctx.pitch_space.as_ref() == Some(ps),
            BarrierScope::Registered(_) => true,
        }
    }
}

// --- Canonical encoding (Appendix D): set-valued fields in canonical order ---

fn push_u64(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// Length-prefixes one element's canonical bytes (self-delimiting).
fn push_elem<T: CanonicalEncode>(out: &mut Vec<u8>, item: &T) {
    let bytes = item.to_canonical_bytes();
    push_u64(out, bytes.len() as u64);
    out.extend_from_slice(&bytes);
}

/// Emits a set-valued field in canonical byte order with **duplicates removed**,
/// so element order and repetition do not affect the encoding — `[A]` and
/// `[A, A]` serialize identically (Appendix D §"Ordered Iteration over Sets and
/// Maps").
fn push_set<T: CanonicalEncode>(out: &mut Vec<u8>, items: &[T]) {
    let mut encoded: Vec<Vec<u8>> = items.iter().map(|i| i.to_canonical_bytes()).collect();
    encoded.sort();
    encoded.dedup();
    push_u64(out, encoded.len() as u64);
    for bytes in encoded {
        push_u64(out, bytes.len() as u64);
        out.extend_from_slice(&bytes);
    }
}

/// Emits an order-significant list (the structure of a condition tree).
fn push_list<T: CanonicalEncode>(out: &mut Vec<u8>, items: &[T]) {
    push_u64(out, items.len() as u64);
    for item in items {
        push_elem(out, item);
    }
}

impl CanonicalEncode for BarrierScopeRegistryId {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.0.to_le_bytes());
    }
}
impl CanonicalEncode for BarrierConditionRegistryId {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.0.to_le_bytes());
    }
}
impl CanonicalEncode for ExtensionRef {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.0.to_le_bytes());
    }
}

impl CanonicalEncode for BarrierScope {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        match self {
            BarrierScope::WholeScore => out.push(0),
            BarrierScope::Region(id) => {
                out.push(1);
                id.encode_canonical(out);
            }
            BarrierScope::StaffInstance(id) => {
                out.push(2);
                id.encode_canonical(out);
            }
            BarrierScope::AnalysisLayer(id) => {
                out.push(3);
                id.encode_canonical(out);
            }
            BarrierScope::ObjectSet(objs) => {
                out.push(4);
                push_set(out, objs);
            }
            BarrierScope::PitchSpace(id) => {
                out.push(5);
                // `PitchSpaceId` is a text catalog id (NFC-normalized at
                // construction, Appendix D §"Text and Unicode"); encode its
                // normalized string, length-prefixed.
                let s = id.to_string();
                push_u64(out, s.len() as u64);
                out.extend_from_slice(s.as_bytes());
            }
            BarrierScope::TuningContext => out.push(6),
            BarrierScope::Registered(id) => {
                out.push(7);
                id.encode_canonical(out);
            }
        }
    }
}

impl CanonicalEncode for BarrierCondition {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        match self {
            BarrierCondition::Always => out.push(0),
            BarrierCondition::ObjectExists(obj) => {
                out.push(1);
                obj.encode_canonical(out);
            }
            BarrierCondition::ObjectHasExtensionData { object, extension } => {
                out.push(2);
                object.encode_canonical(out);
                extension.encode_canonical(out);
            }
            BarrierCondition::All(cs) => {
                out.push(3);
                push_list(out, cs);
            }
            BarrierCondition::Any(cs) => {
                out.push(4);
                push_list(out, cs);
            }
            BarrierCondition::Not(c) => {
                out.push(5);
                push_elem(out, c.as_ref());
            }
            BarrierCondition::Registered(id) => {
                out.push(6);
                id.encode_canonical(out);
            }
        }
    }
}

impl CanonicalEncode for EditBarrier {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        self.scope.encode_canonical(out);
        push_set(out, &self.affected_object_kinds);
        push_set(out, &self.prohibited_operation_kinds);
        self.condition.encode_canonical(out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use epiphany_core::{EventId, RegionId};

    fn ev(raw: u128) -> TypedObjectId {
        TypedObjectId::Event(EventId::from_raw(raw))
    }

    #[test]
    fn prohibits_matching_op_within_object_set() {
        let target = ev(1);
        let ctx = EditContext::default();
        let barrier = EditBarrier {
            scope: BarrierScope::ObjectSet(vec![target]),
            affected_object_kinds: vec![ObjectKind::of(&target)],
            prohibited_operation_kinds: vec![OperationKindTag::DeleteEvent],
            condition: BarrierCondition::Always,
        };
        let oracle = AlwaysLiveOracle;
        // Matching op + object inside the scope set is prohibited.
        assert!(barrier.prohibits_edit(OperationKindTag::DeleteEvent, &target, &ctx, &oracle));
        // A non-prohibited op is allowed.
        assert!(!barrier.prohibits_edit(OperationKindTag::InsertEvent, &target, &ctx, &oracle));
        // An object outside the scope set is allowed.
        assert!(!barrier.prohibits_edit(OperationKindTag::DeleteEvent, &ev(2), &ctx, &oracle));
    }

    #[test]
    fn object_exists_condition_is_evaluated_via_the_oracle() {
        let target = ev(5);
        let barrier = EditBarrier {
            scope: BarrierScope::WholeScore,
            affected_object_kinds: vec![],
            prohibited_operation_kinds: vec![OperationKindTag::DeleteEvent],
            condition: BarrierCondition::ObjectExists(target),
        };
        let ctx = EditContext::default();
        // When the oracle says the object is live, the barrier is active.
        assert!(barrier.prohibits_edit(
            OperationKindTag::DeleteEvent,
            &target,
            &ctx,
            &AlwaysLiveOracle
        ));
        // When it says the object is gone, the barrier does not fire.
        struct Dead;
        impl EditOracle for Dead {
            fn object_exists(&self, _o: &TypedObjectId) -> bool {
                false
            }
            fn has_extension_data(&self, _o: &TypedObjectId, _e: ExtensionRef) -> bool {
                false
            }
        }
        assert!(!barrier.prohibits_edit(OperationKindTag::DeleteEvent, &target, &ctx, &Dead));
    }

    #[test]
    fn region_scope_is_evaluated_precisely() {
        let target = ev(1);
        let region = RegionId::from_raw(7);
        let barrier = EditBarrier {
            scope: BarrierScope::Region(region),
            affected_object_kinds: vec![],
            prohibited_operation_kinds: vec![OperationKindTag::DeleteEvent],
            condition: BarrierCondition::Always,
        };
        // An object inside the barrier's region is prohibited...
        let inside = EditContext {
            region: Some(region),
            ..EditContext::default()
        };
        assert!(barrier.prohibits_edit(
            OperationKindTag::DeleteEvent,
            &target,
            &inside,
            &AlwaysLiveOracle
        ));
        // ...but an object in a different region is NOT (no over-prohibition).
        let outside = EditContext {
            region: Some(RegionId::from_raw(8)),
            ..EditContext::default()
        };
        assert!(!barrier.prohibits_edit(
            OperationKindTag::DeleteEvent,
            &target,
            &outside,
            &AlwaysLiveOracle
        ));
    }

    #[test]
    fn empty_object_kinds_matches_any_kind() {
        let barrier = EditBarrier {
            scope: BarrierScope::WholeScore,
            affected_object_kinds: vec![],
            prohibited_operation_kinds: vec![OperationKindTag::RespellPitch],
            condition: BarrierCondition::Always,
        };
        assert!(barrier.prohibits_edit(
            OperationKindTag::RespellPitch,
            &ev(9),
            &EditContext::default(),
            &AlwaysLiveOracle
        ));
    }

    #[test]
    fn condition_not_always_blocks() {
        let target = ev(3);
        let barrier = EditBarrier {
            scope: BarrierScope::WholeScore,
            affected_object_kinds: vec![],
            prohibited_operation_kinds: vec![OperationKindTag::DeleteEvent],
            condition: BarrierCondition::Not(Box::new(BarrierCondition::Always)),
        };
        // Not(Always) is inactive, so nothing is prohibited.
        assert!(!barrier.prohibits_edit(
            OperationKindTag::DeleteEvent,
            &target,
            &EditContext::default(),
            &AlwaysLiveOracle
        ));
    }

    #[test]
    fn unknown_registered_condition_stays_conservative_through_negation() {
        let condition = BarrierCondition::Not(Box::new(BarrierCondition::Registered(
            BarrierConditionRegistryId(7),
        )));
        assert!(condition.is_active(&AlwaysLiveOracle));
    }

    #[test]
    fn set_encoding_dedups_repeated_kinds() {
        let one = EditBarrier {
            scope: BarrierScope::WholeScore,
            affected_object_kinds: vec![ObjectKind(0)],
            prohibited_operation_kinds: vec![OperationKindTag::DeleteEvent],
            condition: BarrierCondition::Always,
        };
        let repeated = EditBarrier {
            prohibited_operation_kinds: vec![
                OperationKindTag::DeleteEvent,
                OperationKindTag::DeleteEvent,
            ],
            ..one.clone()
        };
        assert_eq!(one.to_canonical_bytes(), repeated.to_canonical_bytes());
    }

    #[test]
    fn canonical_encoding_is_set_order_independent() {
        let mk = |kinds: Vec<OperationKindTag>| EditBarrier {
            scope: BarrierScope::WholeScore,
            affected_object_kinds: vec![ObjectKind(2), ObjectKind(0), ObjectKind(1)],
            prohibited_operation_kinds: kinds,
            condition: BarrierCondition::Always,
        };
        let a = mk(vec![
            OperationKindTag::DeleteEvent,
            OperationKindTag::InsertEvent,
        ]);
        let b = mk(vec![
            OperationKindTag::InsertEvent,
            OperationKindTag::DeleteEvent,
        ]);
        assert_eq!(a.to_canonical_bytes(), b.to_canonical_bytes());

        // A different scope must change the encoding.
        let c = EditBarrier {
            scope: BarrierScope::TuningContext,
            ..a.clone()
        };
        assert_ne!(a.to_canonical_bytes(), c.to_canonical_bytes());
    }
}
