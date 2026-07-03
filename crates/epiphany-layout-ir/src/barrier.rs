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
use epiphany_determinism::{CanonicalDecode, CanonicalEncode, DecodeError};
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

// --- Canonical decoding (the inverse) and the manifest blob codec -----------
//
// PROVISIONAL byte form, pending Binary Format companion ratification (the
// established pattern: define concretely, golden-lock, document for the
// companion). The manifest's `ExtensionDeclaration.edit_barriers` /
// `.affected_object_kinds` fields are opaque bytes to the bundle (Agent D
// preserves them verbatim); this crate — the owner of the barrier types — owns
// what those bytes mean:
//
// * `encode_affected_object_kinds` / `decode_affected_object_kinds`: a
//   canonical SET of [`ObjectKind`]s — `u64` LE count, then per element a
//   `u64` LE length prefix and the element's canonical bytes (2 LE bytes),
//   elements strictly ascending byte-lexicographic, no duplicates (the same
//   `push_set` framing the barrier encoding itself uses).
// * `encode_edit_barriers` / `decode_edit_barriers`: a canonical SET of
//   [`EditBarrier`]s under the identical framing, each element an
//   `EditBarrier`'s canonical bytes.
//
// Decoding is reject-never-normalize (Appendix D discipline): unknown
// discriminants, unsorted or duplicated set elements, non-NFC pitch-space
// strings, over-deep condition trees, and trailing bytes are all typed errors.

/// The maximum [`BarrierCondition`] nesting depth the decoder accepts. The
/// spec places no bound on the recursive condition tree; a decoder needs one
/// so adversarial bytes cannot drive unbounded recursion. 64 levels of
/// `All`/`Any`/`Not` nesting is far beyond any plausible real barrier (the
/// spec's own examples are depth 1–2); this constant is part of the
/// provisional byte contract and is a Binary Format companion candidate.
pub const MAX_CONDITION_DEPTH: usize = 64;

/// Why decoding barrier bytes failed. Construction-side canonical bytes never
/// produce these; any of them means foreign, corrupt, or non-canonical data,
/// which is rejected rather than repaired.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum BarrierDecodeError {
    /// A field ended before its declared or fixed width.
    UnexpectedEof,
    /// A length prefix cannot be represented safely by this process.
    LengthOverflow,
    /// A tagged union carried an unknown discriminant.
    InvalidTag { kind: &'static str, tag: u8 },
    /// An embedded primitive failed its own canonical decoder.
    InvalidValue(&'static str),
    /// A canonical text field was not UTF-8.
    InvalidUtf8,
    /// A canonical text field was not in Unicode NFC (Appendix D §"Text and
    /// Unicode" requires NFC bytes; a non-NFC spelling is rejected, never
    /// silently normalized).
    NotNfc,
    /// A set-valued field was not in strictly ascending canonical byte order
    /// (unsorted, or a duplicate element), or the bytes re-encoded differently.
    NonCanonical(&'static str),
    /// A condition tree nested deeper than [`MAX_CONDITION_DEPTH`].
    ConditionTooDeep,
    /// Bytes remained after the complete value was decoded.
    TrailingBytes,
}

impl core::fmt::Display for BarrierDecodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnexpectedEof => f.write_str("unexpected end of edit-barrier bytes"),
            Self::LengthOverflow => f.write_str("edit-barrier length does not fit usize"),
            Self::InvalidTag { kind, tag } => write!(f, "invalid {kind} tag {tag}"),
            Self::InvalidValue(kind) => write!(f, "invalid canonical {kind}"),
            Self::InvalidUtf8 => f.write_str("invalid UTF-8 in canonical text"),
            Self::NotNfc => f.write_str("canonical text is not NFC-normalized"),
            Self::NonCanonical(what) => write!(f, "edit-barrier {what} is not in canonical form"),
            Self::ConditionTooDeep => write!(
                f,
                "barrier condition nests deeper than {MAX_CONDITION_DEPTH}"
            ),
            Self::TrailingBytes => f.write_str("trailing bytes after edit-barrier value"),
        }
    }
}

impl std::error::Error for BarrierDecodeError {}

impl CanonicalDecode for ObjectKind {
    /// Exactly the 2 little-endian bytes of the wrapped discriminant. Every
    /// `u16` round-trips: the payload is an open discriminant space (a future
    /// core kind or an extension-registered kind is a value, not a decode
    /// branch), so there is no tag to reject here.
    fn decode_canonical(bytes: &[u8]) -> Result<Self, DecodeError> {
        let arr: [u8; 2] = bytes
            .try_into()
            .map_err(|_| DecodeError::UnexpectedLength {
                expected: 2,
                actual: bytes.len(),
            })?;
        Ok(ObjectKind(u16::from_le_bytes(arr)))
    }
}

/// Fixed-width 16-byte little-endian decode shared by the barrier registry-id
/// newtypes (mirroring their `to_le_bytes` encode above).
macro_rules! barrier_u128_le_decode {
    ($name:ident) => {
        impl CanonicalDecode for $name {
            fn decode_canonical(bytes: &[u8]) -> Result<Self, DecodeError> {
                let arr: [u8; 16] =
                    bytes
                        .try_into()
                        .map_err(|_| DecodeError::UnexpectedLength {
                            expected: 16,
                            actual: bytes.len(),
                        })?;
                Ok($name(u128::from_le_bytes(arr)))
            }
        }
    };
}
barrier_u128_le_decode!(BarrierScopeRegistryId);
barrier_u128_le_decode!(BarrierConditionRegistryId);
barrier_u128_le_decode!(ExtensionRef);

type DecodeResult<T> = Result<T, BarrierDecodeError>;

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Reader { bytes, pos: 0 }
    }

    fn take(&mut self, n: usize) -> DecodeResult<&'a [u8]> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or(BarrierDecodeError::LengthOverflow)?;
        if end > self.bytes.len() {
            return Err(BarrierDecodeError::UnexpectedEof);
        }
        let out = &self.bytes[self.pos..end];
        self.pos = end;
        Ok(out)
    }

    fn byte(&mut self) -> DecodeResult<u8> {
        Ok(self.take(1)?[0])
    }

    /// A `u64` little-endian length/count field (the width `push_u64` writes),
    /// converted to `usize`.
    fn len(&mut self) -> DecodeResult<usize> {
        let raw = u64::from_le_bytes(self.take(8)?.try_into().expect("fixed width"));
        usize::try_from(raw).map_err(|_| BarrierDecodeError::LengthOverflow)
    }

    /// A length-prefixed byte field (the form `push_elem`/`push_set` write).
    fn lp_bytes(&mut self) -> DecodeResult<&'a [u8]> {
        let len = self.len()?;
        self.take(len)
    }

    fn finish(self) -> DecodeResult<()> {
        if self.pos == self.bytes.len() {
            Ok(())
        } else {
            Err(BarrierDecodeError::TrailingBytes)
        }
    }
}

/// Runs `decode` over exactly `bytes` — trailing bytes are an error.
fn exact<T>(
    bytes: &[u8],
    decode: impl FnOnce(&mut Reader<'_>) -> DecodeResult<T>,
) -> DecodeResult<T> {
    let mut reader = Reader::new(bytes);
    let value = decode(&mut reader)?;
    reader.finish()?;
    Ok(value)
}

/// Reads a `push_set`-framed set: a count, then length-prefixed elements that
/// MUST be strictly ascending in raw byte order (the sorted, deduplicated form
/// the encoder emits). Unsorted or duplicated elements are rejected as
/// non-canonical, never re-sorted.
fn read_set<'a, T>(
    reader: &mut Reader<'a>,
    what: &'static str,
    mut decode_elem: impl FnMut(&'a [u8]) -> DecodeResult<T>,
) -> DecodeResult<Vec<T>> {
    let count = reader.len()?;
    let mut values = Vec::with_capacity(count.min(1024));
    let mut prev: Option<&[u8]> = None;
    for _ in 0..count {
        let elem = reader.lp_bytes()?;
        if let Some(prev) = prev {
            if prev >= elem {
                return Err(BarrierDecodeError::NonCanonical(what));
            }
        }
        prev = Some(elem);
        values.push(decode_elem(elem)?);
    }
    Ok(values)
}

/// Streams one `TypedObjectId` (variable width: 2-byte discriminant, then 16
/// payload bytes — or 32 for the `Registered` variant, discriminant 27).
fn read_typed_object_id(reader: &mut Reader<'_>) -> DecodeResult<TypedObjectId> {
    let tag_bytes = reader
        .bytes
        .get(reader.pos..reader.pos.saturating_add(2))
        .ok_or(BarrierDecodeError::UnexpectedEof)?;
    let tag = u16::from_be_bytes(tag_bytes.try_into().expect("two bytes"));
    let width = if tag == 27 { 34 } else { 18 };
    TypedObjectId::decode_canonical(reader.take(width)?)
        .map_err(|_| BarrierDecodeError::InvalidValue("TypedObjectId"))
}

fn read_u128_le(reader: &mut Reader<'_>) -> DecodeResult<u128> {
    Ok(u128::from_le_bytes(
        reader.take(16)?.try_into().expect("fixed width"),
    ))
}

/// Streams one 16-byte big-endian graph identifier (the `graph_id!` canonical
/// form `RegionId`/`StaffInstanceId`/`AnalysisLayerId` share).
fn read_graph_id<T: CanonicalDecode>(
    reader: &mut Reader<'_>,
    name: &'static str,
) -> DecodeResult<T> {
    T::decode_canonical(reader.take(16)?).map_err(|_| BarrierDecodeError::InvalidValue(name))
}

fn read_scope(reader: &mut Reader<'_>) -> DecodeResult<BarrierScope> {
    match reader.byte()? {
        0 => Ok(BarrierScope::WholeScore),
        1 => Ok(BarrierScope::Region(read_graph_id(reader, "RegionId")?)),
        2 => Ok(BarrierScope::StaffInstance(read_graph_id(
            reader,
            "StaffInstanceId",
        )?)),
        3 => Ok(BarrierScope::AnalysisLayer(read_graph_id(
            reader,
            "AnalysisLayerId",
        )?)),
        4 => Ok(BarrierScope::ObjectSet(read_set(
            reader,
            "object set",
            |elem| {
                TypedObjectId::decode_canonical(elem)
                    .map_err(|_| BarrierDecodeError::InvalidValue("TypedObjectId"))
            },
        )?)),
        5 => {
            let raw = reader.lp_bytes()?;
            let s = core::str::from_utf8(raw).map_err(|_| BarrierDecodeError::InvalidUtf8)?;
            // `PitchSpaceId::new` NFC-normalizes; canonical bytes MUST already
            // be NFC, so a spelling the constructor would change is rejected
            // rather than silently normalized (re-encoding it would differ).
            let id = PitchSpaceId::new(s);
            if id.as_str() != s {
                return Err(BarrierDecodeError::NotNfc);
            }
            Ok(BarrierScope::PitchSpace(id))
        }
        6 => Ok(BarrierScope::TuningContext),
        7 => Ok(BarrierScope::Registered(BarrierScopeRegistryId(
            read_u128_le(reader)?,
        ))),
        tag => Err(BarrierDecodeError::InvalidTag {
            kind: "BarrierScope",
            tag,
        }),
    }
}

fn read_condition(reader: &mut Reader<'_>, depth: usize) -> DecodeResult<BarrierCondition> {
    if depth > MAX_CONDITION_DEPTH {
        return Err(BarrierDecodeError::ConditionTooDeep);
    }
    match reader.byte()? {
        0 => Ok(BarrierCondition::Always),
        1 => Ok(BarrierCondition::ObjectExists(read_typed_object_id(
            reader,
        )?)),
        2 => Ok(BarrierCondition::ObjectHasExtensionData {
            object: read_typed_object_id(reader)?,
            extension: ExtensionRef(read_u128_le(reader)?),
        }),
        3 => Ok(BarrierCondition::All(read_condition_list(reader, depth)?)),
        4 => Ok(BarrierCondition::Any(read_condition_list(reader, depth)?)),
        5 => {
            let inner = exact(reader.lp_bytes()?, |r| read_condition(r, depth + 1))?;
            Ok(BarrierCondition::Not(Box::new(inner)))
        }
        6 => Ok(BarrierCondition::Registered(BarrierConditionRegistryId(
            read_u128_le(reader)?,
        ))),
        tag => Err(BarrierDecodeError::InvalidTag {
            kind: "BarrierCondition",
            tag,
        }),
    }
}

/// Reads a `push_list`-framed condition list (order-significant — the
/// *structure* of an `All`/`Any` tree, not a set).
fn read_condition_list(
    reader: &mut Reader<'_>,
    depth: usize,
) -> DecodeResult<Vec<BarrierCondition>> {
    let count = reader.len()?;
    let mut conditions = Vec::with_capacity(count.min(1024));
    for _ in 0..count {
        conditions.push(exact(reader.lp_bytes()?, |r| read_condition(r, depth + 1))?);
    }
    Ok(conditions)
}

fn read_barrier(reader: &mut Reader<'_>) -> DecodeResult<EditBarrier> {
    Ok(EditBarrier {
        scope: read_scope(reader)?,
        affected_object_kinds: read_set(reader, "affected object kinds", |elem| {
            ObjectKind::decode_canonical(elem)
                .map_err(|_| BarrierDecodeError::InvalidValue("ObjectKind"))
        })?,
        prohibited_operation_kinds: read_set(reader, "prohibited operation kinds", |elem| {
            OperationKindTag::decode_canonical(elem).map_err(|err| match (err, elem.first()) {
                (DecodeError::MalformedDomainTag, Some(&tag)) => BarrierDecodeError::InvalidTag {
                    kind: "OperationKindTag",
                    tag,
                },
                _ => BarrierDecodeError::InvalidValue("OperationKindTag"),
            })
        })?,
        condition: read_condition(reader, 1)?,
    })
}

impl EditBarrier {
    /// Decodes exactly one barrier from its canonical bytes (the inverse of
    /// [`CanonicalEncode`]); trailing bytes and every non-canonical form are
    /// rejected. The manifest blob form is [`decode_edit_barriers`].
    pub fn decode_canonical_bytes(bytes: &[u8]) -> DecodeResult<EditBarrier> {
        let barrier = exact(bytes, read_barrier)?;
        // Belt and braces: canonical decode admits exactly the encoder's image,
        // so the round-trip must be byte-identical.
        if barrier.to_canonical_bytes() != bytes {
            return Err(BarrierDecodeError::NonCanonical("barrier bytes"));
        }
        Ok(barrier)
    }
}

/// Encodes a set of edit barriers to the canonical blob stored in
/// `ExtensionDeclaration.edit_barriers`: `push_set` framing (`u64` LE count,
/// then per barrier a `u64` LE length prefix and the barrier's canonical
/// bytes), sorted ascending by encoded bytes with duplicates removed, so the
/// blob is order- and repetition-independent.
pub fn encode_edit_barriers(barriers: &[EditBarrier]) -> Vec<u8> {
    let mut out = Vec::new();
    push_set(&mut out, barriers);
    out
}

/// Decodes an `ExtensionDeclaration.edit_barriers` blob (the inverse of
/// [`encode_edit_barriers`]). Rejects unsorted/duplicated barriers, unknown
/// discriminants anywhere in the tree, non-NFC pitch-space text, over-deep
/// condition trees, and trailing bytes.
pub fn decode_edit_barriers(bytes: &[u8]) -> DecodeResult<Vec<EditBarrier>> {
    let barriers = exact(bytes, |reader| {
        read_set(
            reader,
            "edit-barrier set",
            EditBarrier::decode_canonical_bytes,
        )
    })?;
    Ok(barriers)
}

/// Encodes a set of object kinds to the canonical blob stored in
/// `ExtensionDeclaration.affected_object_kinds` (the same `push_set` framing
/// as [`encode_edit_barriers`]; each element is the kind's 2 LE bytes).
pub fn encode_affected_object_kinds(kinds: &[ObjectKind]) -> Vec<u8> {
    let mut out = Vec::new();
    push_set(&mut out, kinds);
    out
}

/// Decodes an `ExtensionDeclaration.affected_object_kinds` blob (the inverse
/// of [`encode_affected_object_kinds`]).
pub fn decode_affected_object_kinds(bytes: &[u8]) -> DecodeResult<Vec<ObjectKind>> {
    exact(bytes, |reader| {
        read_set(reader, "affected-object-kind set", |elem| {
            ObjectKind::decode_canonical(elem)
                .map_err(|_| BarrierDecodeError::InvalidValue("ObjectKind"))
        })
    })
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

    // --- Decode mirrors + the manifest blob codec ---------------------------

    fn u64le(n: u64) -> Vec<u8> {
        n.to_le_bytes().to_vec()
    }

    /// `push_set` framing assembled by hand, so the reject tests can produce
    /// deliberately non-canonical framings the encoder never emits.
    fn set_blob(elems: &[Vec<u8>]) -> Vec<u8> {
        let mut out = u64le(elems.len() as u64);
        for elem in elems {
            out.extend(u64le(elem.len() as u64));
            out.extend(elem);
        }
        out
    }

    #[test]
    fn affected_object_kinds_blob_bytes_are_golden() {
        // GOLDEN LOCK (provisional canonical form, pending Binary Format
        // companion ratification): u64 LE count, then per element a u64 LE
        // length prefix and the ObjectKind's 2 LE bytes, ascending, deduped.
        let blob = encode_affected_object_kinds(&[ObjectKind(1), ObjectKind(0), ObjectKind(1)]);
        #[rustfmt::skip]
        const GOLDEN: [u8; 28] = [
            2, 0, 0, 0, 0, 0, 0, 0,       // count = 2 (the duplicate collapses)
            2, 0, 0, 0, 0, 0, 0, 0, 0, 0, // len 2, ObjectKind(0)
            2, 0, 0, 0, 0, 0, 0, 0, 1, 0, // len 2, ObjectKind(1)
        ];
        assert_eq!(blob, GOLDEN);
        assert_eq!(
            decode_affected_object_kinds(&GOLDEN).unwrap(),
            vec![ObjectKind(0), ObjectKind(1)]
        );
    }

    #[test]
    fn edit_barriers_blob_bytes_are_golden() {
        // GOLDEN LOCK (provisional canonical form, pending Binary Format
        // companion ratification): the set framing wraps each barrier's
        // canonical bytes — scope, affected-kind set, prohibited-tag set,
        // condition, in that order.
        let barrier = EditBarrier {
            scope: BarrierScope::WholeScore,
            affected_object_kinds: vec![ObjectKind(1)],
            prohibited_operation_kinds: vec![OperationKindTag::DeleteEvent],
            condition: BarrierCondition::Always,
        };
        let blob = encode_edit_barriers(std::slice::from_ref(&barrier));
        #[rustfmt::skip]
        const GOLDEN: [u8; 53] = [
            1, 0, 0, 0, 0, 0, 0, 0,        // set count = 1
            37, 0, 0, 0, 0, 0, 0, 0,       // barrier byte length
            0,                             // scope: WholeScore
            1, 0, 0, 0, 0, 0, 0, 0,        // affected kinds: count = 1
            2, 0, 0, 0, 0, 0, 0, 0, 1, 0,  //   len 2, ObjectKind(1)
            1, 0, 0, 0, 0, 0, 0, 0,        // prohibited tags: count = 1
            1, 0, 0, 0, 0, 0, 0, 0, 1,     //   len 1, DeleteEvent (tag 1)
            0,                             // condition: Always
        ];
        assert_eq!(blob, GOLDEN);
        assert_eq!(decode_edit_barriers(&GOLDEN).unwrap(), vec![barrier]);
    }

    #[test]
    fn every_scope_and_condition_variant_round_trips_byte_identically() {
        use epiphany_core::{AnalysisLayerId, StaffInstanceId};
        let scopes = vec![
            BarrierScope::WholeScore,
            BarrierScope::Region(RegionId::from_raw(7)),
            BarrierScope::StaffInstance(StaffInstanceId::from_raw(8)),
            BarrierScope::AnalysisLayer(AnalysisLayerId::from_raw(9)),
            // In canonical (ascending) order: decode admits only the canonical
            // image, so the round-trip compares structurally equal.
            BarrierScope::ObjectSet(vec![ev(1), ev(2)]),
            BarrierScope::PitchSpace(PitchSpaceId::new("cmn-12")),
            BarrierScope::TuningContext,
            BarrierScope::Registered(BarrierScopeRegistryId(10)),
        ];
        let conditions = vec![
            BarrierCondition::Always,
            BarrierCondition::ObjectExists(ev(3)),
            BarrierCondition::ObjectHasExtensionData {
                object: ev(4),
                extension: ExtensionRef(11),
            },
            BarrierCondition::All(vec![
                BarrierCondition::Always,
                BarrierCondition::ObjectExists(ev(5)),
            ]),
            BarrierCondition::Any(vec![BarrierCondition::Not(Box::new(
                BarrierCondition::Always,
            ))]),
            BarrierCondition::Not(Box::new(BarrierCondition::Registered(
                BarrierConditionRegistryId(12),
            ))),
            BarrierCondition::Registered(BarrierConditionRegistryId(13)),
        ];
        let barriers: Vec<EditBarrier> = scopes
            .into_iter()
            .zip(conditions.into_iter().cycle())
            .map(|(scope, condition)| EditBarrier {
                scope,
                affected_object_kinds: vec![ObjectKind(0), ObjectKind(4)],
                prohibited_operation_kinds: vec![
                    OperationKindTag::DeleteEvent,
                    OperationKindTag::Registered(epiphany_ops::OperationKindRegistryId(99)),
                ],
                condition,
            })
            .collect();
        // Single-barrier decode mirrors encode exactly.
        for barrier in &barriers {
            let bytes = barrier.to_canonical_bytes();
            let decoded = EditBarrier::decode_canonical_bytes(&bytes).unwrap();
            assert_eq!(&decoded, barrier);
            assert_eq!(decoded.to_canonical_bytes(), bytes);
        }
        // The blob is a canonical set: order- and repetition-independent, and
        // decode → re-encode is byte-identical.
        let blob = encode_edit_barriers(&barriers);
        let mut reversed: Vec<EditBarrier> = barriers.iter().rev().cloned().collect();
        reversed.push(barriers[0].clone());
        assert_eq!(encode_edit_barriers(&reversed), blob);
        let decoded = decode_edit_barriers(&blob).unwrap();
        assert_eq!(decoded.len(), barriers.len());
        assert_eq!(encode_edit_barriers(&decoded), blob);
    }

    #[test]
    fn decode_rejects_unknown_discriminants() {
        // Scope tag 8 is one past the vocabulary.
        let mut bytes = vec![8u8];
        bytes.extend(set_blob(&[]));
        bytes.extend(set_blob(&[]));
        bytes.push(0);
        assert_eq!(
            EditBarrier::decode_canonical_bytes(&bytes),
            Err(BarrierDecodeError::InvalidTag {
                kind: "BarrierScope",
                tag: 8
            })
        );
        // Condition tag 7 is one past the vocabulary.
        let mut bytes = vec![0u8];
        bytes.extend(set_blob(&[]));
        bytes.extend(set_blob(&[]));
        bytes.push(7);
        assert_eq!(
            EditBarrier::decode_canonical_bytes(&bytes),
            Err(BarrierDecodeError::InvalidTag {
                kind: "BarrierCondition",
                tag: 7
            })
        );
        // Operation-kind tag 28 is one past the v1 vocabulary (the Phase-3
        // ops tranche appended 24..=27; encodings are append-only).
        let mut bytes = vec![0u8];
        bytes.extend(set_blob(&[]));
        bytes.extend(set_blob(&[vec![28u8]]));
        bytes.push(0);
        assert_eq!(
            EditBarrier::decode_canonical_bytes(&bytes),
            Err(BarrierDecodeError::InvalidTag {
                kind: "OperationKindTag",
                tag: 28
            })
        );
    }

    #[test]
    fn decode_rejects_non_nfc_pitch_space_text() {
        // scope: PitchSpace(tag 5) carrying a decomposed "café" — canonically
        // equivalent to the NFC form but not byte-canonical. Rejected, never
        // silently normalized.
        let decomposed = "cafe\u{0301}";
        let mut bytes = vec![5u8];
        bytes.extend(u64le(decomposed.len() as u64));
        bytes.extend(decomposed.as_bytes());
        bytes.extend(set_blob(&[]));
        bytes.extend(set_blob(&[]));
        bytes.push(0);
        assert_eq!(
            EditBarrier::decode_canonical_bytes(&bytes),
            Err(BarrierDecodeError::NotNfc)
        );
        // The NFC spelling of the same name decodes (and re-encodes) fine.
        let nfc = EditBarrier {
            scope: BarrierScope::PitchSpace(PitchSpaceId::new("caf\u{00e9}")),
            affected_object_kinds: vec![],
            prohibited_operation_kinds: vec![],
            condition: BarrierCondition::Always,
        };
        let round = EditBarrier::decode_canonical_bytes(&nfc.to_canonical_bytes()).unwrap();
        assert_eq!(round, nfc);
    }

    #[test]
    fn decode_rejects_unsorted_and_duplicated_sets_and_trailing_bytes() {
        // Kinds out of ascending byte order.
        let unsorted = {
            let mut out = vec![0u8];
            out.extend(set_blob(&[vec![1, 0], vec![0, 0]]));
            out.extend(set_blob(&[]));
            out.push(0);
            out
        };
        assert_eq!(
            EditBarrier::decode_canonical_bytes(&unsorted),
            Err(BarrierDecodeError::NonCanonical("affected object kinds"))
        );
        // A duplicated element (sorted but not strictly ascending).
        let duplicated = {
            let mut out = vec![0u8];
            out.extend(set_blob(&[vec![0, 0], vec![0, 0]]));
            out.extend(set_blob(&[]));
            out.push(0);
            out
        };
        assert_eq!(
            EditBarrier::decode_canonical_bytes(&duplicated),
            Err(BarrierDecodeError::NonCanonical("affected object kinds"))
        );
        // Trailing bytes after a complete blob / a complete barrier.
        let blob = encode_edit_barriers(&[EditBarrier {
            scope: BarrierScope::WholeScore,
            affected_object_kinds: vec![],
            prohibited_operation_kinds: vec![],
            condition: BarrierCondition::Always,
        }]);
        let mut trailing = blob.clone();
        trailing.push(0);
        assert_eq!(
            decode_edit_barriers(&trailing),
            Err(BarrierDecodeError::TrailingBytes)
        );
        // Truncation is an error, at every prefix length.
        for cut in 0..blob.len() {
            assert!(decode_edit_barriers(&blob[..cut]).is_err());
        }
        let mut kinds_trailing = encode_affected_object_kinds(&[ObjectKind(3)]);
        kinds_trailing.push(0);
        assert_eq!(
            decode_affected_object_kinds(&kinds_trailing),
            Err(BarrierDecodeError::TrailingBytes)
        );
    }

    #[test]
    fn decode_bounds_condition_recursion_depth() {
        let nested_nots = |n: usize| {
            let mut condition = BarrierCondition::Always;
            for _ in 0..n {
                condition = BarrierCondition::Not(Box::new(condition));
            }
            EditBarrier {
                scope: BarrierScope::WholeScore,
                affected_object_kinds: vec![],
                prohibited_operation_kinds: vec![],
                condition,
            }
        };
        // MAX_CONDITION_DEPTH - 1 wrappers put the innermost leaf exactly at
        // the bound: accepted.
        let at_bound = nested_nots(MAX_CONDITION_DEPTH - 1);
        assert_eq!(
            EditBarrier::decode_canonical_bytes(&at_bound.to_canonical_bytes()).unwrap(),
            at_bound
        );
        // One wrapper more nests past the bound: rejected.
        let past_bound = nested_nots(MAX_CONDITION_DEPTH);
        assert_eq!(
            EditBarrier::decode_canonical_bytes(&past_bound.to_canonical_bytes()),
            Err(BarrierDecodeError::ConditionTooDeep)
        );
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
