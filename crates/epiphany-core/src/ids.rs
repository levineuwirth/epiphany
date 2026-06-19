//! The typed identifier family (Chapter 5 §"Identifiers").
//!
//! Every named object in the score graph carries a typed 128-bit identifier
//! composed of a 64-bit [`ReplicaId`] and a 64-bit monotonic counter local to
//! that replica (Chapter 5 §"Identifier Generation"). Each object *kind* has
//! its own newtype so that cross-kind confusion is a compile-time error
//! (Chapter 5 §"Design Principles": "Typed identifiers").
//!
//! ## Canonical byte form
//!
//! Appendix D §"Ordered Iteration" fixes the canonical byte form of every
//! typed identifier: *16 bytes — 8-byte replica, 8-byte counter, big-endian* —
//! and the canonical total order is lexicographic ascending on those bytes.
//! We store each identifier as a single `u128` equal to
//! `(replica << 64) | counter`; then [`u128::to_be_bytes`] *is* the canonical
//! 16-byte form, and the derived numeric `Ord` on the `u128` is exactly the
//! lexicographic byte order. Identity is therefore exact and never tolerant
//! (Appendix D §"Tolerance Classes").
//!
//! ## System-derived identifiers
//!
//! [`ReplicaId::SYSTEM_DERIVED`] (`0xffff_ffff_ffff_ffff`) is reserved for
//! deterministically-derived system identifiers (system-promoted voices,
//! content-derived ids). User-authored replicas must never use it. The 64-bit
//! counter of such an identifier is `trunc64(BLAKE3(domain_tag || inputs))`
//! via [`epiphany_determinism::derive_system_counter`]; [`derive_system_id`]
//! wraps that into any typed identifier.

use epiphany_determinism::{
    derive_system_counter, CanonicalByteOrder, CanonicalDecode, CanonicalEncode, DecodeError,
    SystemDomainTag,
};

/// A replica identifier: the 64-bit high half of every graph identifier
/// (Chapter 5 §"Identifier Generation"). Generated once at score creation with
/// at least 64 bits of CSPRNG entropy.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ReplicaId(pub u64);

impl ReplicaId {
    /// Reserved replica identifier for deterministically-derived system
    /// identifiers (system-promoted voices, content-derived conflict ids,
    /// future deterministic synthetic identifiers). User-authored replicas
    /// **must not** use this value (Chapter 5 §"System-Derived Identifier
    /// Namespace").
    pub const SYSTEM_DERIVED: ReplicaId = ReplicaId(0xffff_ffff_ffff_ffff);

    /// Whether this is the reserved [`Self::SYSTEM_DERIVED`] namespace.
    #[inline]
    pub const fn is_system_derived(self) -> bool {
        self.0 == Self::SYSTEM_DERIVED.0
    }

    /// Wraps raw entropy into a replica identifier, rejecting the reserved
    /// [`Self::SYSTEM_DERIVED`] value (Chapter 5: "Implementations creating a
    /// new score MUST reject this value … and MUST regenerate"). Returns `None`
    /// if the entropy happens to land on the reserved value so the caller
    /// re-draws — exactly what [`Self::generate`] does.
    #[inline]
    pub fn from_entropy(bytes: [u8; 8]) -> Option<Self> {
        let v = ReplicaId(u64::from_le_bytes(bytes));
        if v.is_system_derived() {
            None
        } else {
            Some(v)
        }
    }

    /// Generates a fresh replica identifier from the platform CSPRNG
    /// (QUICKSTART decision 1: `getrandom`), re-drawing until the value is not
    /// the reserved [`Self::SYSTEM_DERIVED`] namespace. This is the only
    /// sanctioned use of platform randomness in the core (Appendix D
    /// §"Randomness"); the entropy enters canonical state only via the
    /// identifiers it seeds.
    ///
    /// # Panics
    /// Panics only if the platform entropy source itself fails, which on a
    /// conforming platform does not happen; identifier minting cannot proceed
    /// without it.
    pub fn generate() -> Self {
        loop {
            let mut bytes = [0u8; 8];
            getrandom::getrandom(&mut bytes).expect("platform CSPRNG unavailable");
            if let Some(id) = Self::from_entropy(bytes) {
                return id;
            }
        }
    }

    /// The canonical 8 big-endian bytes.
    #[inline]
    pub const fn to_be_bytes(self) -> [u8; 8] {
        self.0.to_be_bytes()
    }
}

impl core::fmt::Debug for ReplicaId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.is_system_derived() {
            f.write_str("ReplicaId(SYSTEM_DERIVED)")
        } else {
            write!(f, "ReplicaId({:016x})", self.0)
        }
    }
}

impl CanonicalEncode for ReplicaId {
    #[inline]
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.to_be_bytes());
    }
}
impl CanonicalDecode for ReplicaId {
    #[inline]
    fn decode_canonical(bytes: &[u8]) -> Result<Self, DecodeError> {
        let arr: [u8; 8] = bytes
            .try_into()
            .map_err(|_| DecodeError::UnexpectedLength {
                expected: 8,
                actual: bytes.len(),
            })?;
        Ok(ReplicaId(u64::from_be_bytes(arr)))
    }
}
impl CanonicalByteOrder for ReplicaId {}

/// Behaviour shared by every typed 128-bit graph identifier. Implemented by
/// the `graph_id!` macro; consumed by [`IdentityContext::mint`] and
/// [`derive_system_id`] so minting is generic over the identifier kind.
pub trait GraphId: Copy + Eq + Ord + core::hash::Hash {
    /// Builds an identifier from its replica and counter parts.
    fn from_parts(replica: ReplicaId, counter: u64) -> Self;
    /// The replica half.
    fn replica(self) -> ReplicaId;
    /// The counter half.
    fn counter(self) -> u64;
    /// The whole identifier as a `u128` (`(replica << 64) | counter`).
    fn as_u128(self) -> u128;
    /// The canonical 16-byte big-endian form (Appendix D §"Ordered Iteration").
    fn canonical_bytes(self) -> [u8; 16];
}

/// Defines a typed 128-bit graph identifier newtype over `u128`, with the
/// canonical byte form and ordering fixed by Appendix D. The spec writes each
/// of these as e.g. `pub struct EventId(u128)`; this macro keeps that shape
/// while sharing the replica/counter logic and the canonical-encoding impls.
macro_rules! graph_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
        pub struct $name(pub u128);

        impl $name {
            /// Builds the identifier from a replica and a counter. The replica
            /// occupies the high 64 bits so the numeric order matches the
            /// canonical byte order.
            #[inline]
            pub const fn new(replica: ReplicaId, counter: u64) -> Self {
                $name(((replica.0 as u128) << 64) | (counter as u128))
            }

            /// Wraps a raw `u128` (e.g. when decoding).
            #[inline]
            pub const fn from_raw(raw: u128) -> Self {
                $name(raw)
            }

            /// The raw `u128`.
            #[inline]
            pub const fn as_u128(self) -> u128 {
                self.0
            }

            /// The replica half (high 64 bits).
            #[inline]
            pub const fn replica(self) -> ReplicaId {
                ReplicaId((self.0 >> 64) as u64)
            }

            /// The counter half (low 64 bits).
            #[inline]
            pub const fn counter(self) -> u64 {
                self.0 as u64
            }

            /// The canonical 16-byte big-endian form: 8-byte replica then
            /// 8-byte counter (Appendix D §"Ordered Iteration").
            #[inline]
            pub const fn canonical_bytes(self) -> [u8; 16] {
                self.0.to_be_bytes()
            }
        }

        impl GraphId for $name {
            #[inline]
            fn from_parts(replica: ReplicaId, counter: u64) -> Self {
                $name::new(replica, counter)
            }
            #[inline]
            fn replica(self) -> ReplicaId {
                $name::replica(self)
            }
            #[inline]
            fn counter(self) -> u64 {
                $name::counter(self)
            }
            #[inline]
            fn as_u128(self) -> u128 {
                self.0
            }
            #[inline]
            fn canonical_bytes(self) -> [u8; 16] {
                $name::canonical_bytes(self)
            }
        }

        impl core::fmt::Debug for $name {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(
                    f,
                    concat!(stringify!($name), "({:016x}:{:016x})"),
                    (self.0 >> 64) as u64,
                    self.0 as u64
                )
            }
        }

        impl CanonicalEncode for $name {
            #[inline]
            fn encode_canonical(&self, out: &mut Vec<u8>) {
                out.extend_from_slice(&self.canonical_bytes());
            }
        }
        impl CanonicalDecode for $name {
            #[inline]
            fn decode_canonical(bytes: &[u8]) -> Result<Self, DecodeError> {
                let arr: [u8; 16] =
                    bytes.try_into().map_err(|_| DecodeError::UnexpectedLength {
                        expected: 16,
                        actual: bytes.len(),
                    })?;
                Ok($name(u128::from_be_bytes(arr)))
            }
        }
        // Canonical order *is* byte order for typed identifiers (Appendix D).
        impl CanonicalByteOrder for $name {}
    };
}

graph_id!(
    /// Identifies a rhythmic event in the [`crate::EventArena`].
    EventId
);
graph_id!(
    /// Identifies a pitch embedded in an event (Chapter 2 §"Pitch Identifiers").
    PitchId
);
graph_id!(
    /// Identifies a polyphonic voice within a staff instance.
    VoiceId
);
graph_id!(
    /// Identifies a global, abstract staff (Chapter 5 §"Staves: Identity Versus
    /// Instance").
    StaffId
);
graph_id!(
    /// Identifies a region-local manifestation of a [`StaffId`].
    StaffInstanceId
);
graph_id!(
    /// Identifies a staff grouping (grand staff, bracket, choral group).
    StaffGroupId
);
graph_id!(
    /// Identifies a region of the canvas.
    RegionId
);
graph_id!(
    /// Identifies an abstract instrument definition.
    InstrumentId
);
graph_id!(
    /// Identifies a part-extraction view definition.
    PartDefinitionId
);
graph_id!(
    /// Identifies a measure (belongs to exactly one staff instance).
    MeasureId
);
graph_id!(
    /// Identifies a barline-alignment group.
    BarlineAlignmentGroupId
);
graph_id!(
    /// Identifies a slur / phrase mark.
    SlurId
);
graph_id!(
    /// Identifies a tie.
    TieId
);
graph_id!(
    /// Identifies a beam.
    BeamId
);
graph_id!(
    /// Identifies a spanner (hairpin, octave line, pedal, …).
    SpannerId
);
graph_id!(
    /// Identifies a tuplet grouping object (Chapter 3 §"Tuplets").
    TupletId
);
graph_id!(
    /// Identifies a point marker (rehearsal mark, segno, …).
    MarkerId
);
graph_id!(
    /// Identifies an analytical annotation.
    AnalyticalAnnotationId
);
graph_id!(
    /// Identifies a review-mode comment thread.
    CommentId
);
graph_id!(
    /// Identifies a repeat structure (simple repeat, da capo, volta).
    RepeatStructureId
);
graph_id!(
    /// Identifies a lyric line.
    LyricLineId
);
graph_id!(
    /// Identifies a chord symbol.
    ChordSymbolId
);
graph_id!(
    /// Identifies a graphic object in the canvas's graphic storage.
    GraphicObjectId
);
graph_id!(
    /// Identifies a multi-object graphic gesture.
    GraphicGestureId
);
graph_id!(
    /// Identifies a time signature object.
    TimeSignatureId
);
graph_id!(
    /// Identifies an analysis layer (Chapter 5 §"Analysis Layers and Views").
    AnalysisLayerId
);
graph_id!(
    /// Identifies a view definition (Chapter 5 §"Views").
    ViewId
);
graph_id!(
    /// Identifies a transaction grouping of operations (Chapter 6).
    TransactionId
);
graph_id!(
    /// Identifies a diagnostic integrity anomaly (Chapter 5
    /// §"System-Derived Counter Collisions").
    IntegrityAnomalyId
);
graph_id!(
    /// Identifies an extension-introduced object kind, used by
    /// [`TypedObjectId::Registered`].
    ObjectKindRegistryId
);

/// The stable identity of an operation (Chapter 6 §"Operation Identity and
/// Stamps"): a replica plus an authoring counter. Defined here, with the rest
/// of the identifier family, because graph types reference it
/// (notably [`crate::VoiceOrigin::SystemPromoted`]); `epiphany-ops` (Agent C)
/// builds the operation *semantics* on top of it.
///
/// Identity is fixed at authoring time and never changes under reordering,
/// retransmission, or merging. Ordering is `(replica, counter)`, i.e. the same
/// lexicographic order as the typed graph identifiers.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct OperationId {
    /// Authoring replica.
    pub replica: ReplicaId,
    /// Authoring counter, monotonic within the replica.
    pub counter: u64,
}

impl OperationId {
    /// Builds an operation identifier.
    #[inline]
    pub const fn new(replica: ReplicaId, counter: u64) -> Self {
        OperationId { replica, counter }
    }

    /// The canonical 16-byte big-endian form: 8-byte replica then 8-byte
    /// counter, matching the typed-identifier convention (Appendix D).
    #[inline]
    pub fn canonical_bytes(self) -> [u8; 16] {
        let mut out = [0u8; 16];
        out[0..8].copy_from_slice(&self.replica.to_be_bytes());
        out[8..16].copy_from_slice(&self.counter.to_be_bytes());
        out
    }
}

impl core::fmt::Debug for OperationId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "OperationId({:016x}:{:016x})",
            self.replica.0, self.counter
        )
    }
}

impl CanonicalEncode for OperationId {
    #[inline]
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.canonical_bytes());
    }
}
impl CanonicalDecode for OperationId {
    #[inline]
    fn decode_canonical(bytes: &[u8]) -> Result<Self, DecodeError> {
        let arr: [u8; 16] = bytes
            .try_into()
            .map_err(|_| DecodeError::UnexpectedLength {
                expected: 16,
                actual: bytes.len(),
            })?;
        let mut r = [0u8; 8];
        let mut c = [0u8; 8];
        r.copy_from_slice(&arr[0..8]);
        c.copy_from_slice(&arr[8..16]);
        Ok(OperationId::new(
            ReplicaId(u64::from_be_bytes(r)),
            u64::from_be_bytes(c),
        ))
    }
}
impl CanonicalByteOrder for OperationId {}

/// A tagged identifier over every typed identifier kind in the score graph
/// (Chapter 5 §"Identifiers"). Used wherever an object is referenced
/// generically: cross-cutting endpoints, conflict `affected_objects`, repair
/// records, edit barriers.
///
/// The variant tag is part of canonical content: distinct variants with the
/// same underlying `u128` are distinct `TypedObjectId`s. The canonical byte
/// form is a 16-bit big-endian discriminant followed by the payload's
/// canonical bytes (Chapter 5 `TypedObjectId::canonical_bytes`).
///
/// **Discriminant assignment** (a prototype decision; see `DECISIONS.md`): the
/// spec fixes the *shape* of the encoding but not the numeric discriminant per
/// variant. We assign them by declaration order, starting at 0, with
/// [`TypedObjectId::Registered`] last. Because these values enter canonical
/// state, the choice is recorded as a Pass 11 candidate for the spec to pin.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum TypedObjectId {
    Event(EventId),
    Pitch(PitchId),
    Voice(VoiceId),
    Staff(StaffId),
    StaffInstance(StaffInstanceId),
    StaffGroup(StaffGroupId),
    Region(RegionId),
    Instrument(InstrumentId),
    PartDefinition(PartDefinitionId),
    Measure(MeasureId),
    BarlineAlignmentGroup(BarlineAlignmentGroupId),
    Slur(SlurId),
    Tie(TieId),
    Beam(BeamId),
    Spanner(SpannerId),
    Marker(MarkerId),
    AnalyticalAnnotation(AnalyticalAnnotationId),
    Comment(CommentId),
    GraphicObject(GraphicObjectId),
    GraphicGesture(GraphicGestureId),
    TimeSignature(TimeSignatureId),
    AnalysisLayer(AnalysisLayerId),
    Tuplet(TupletId),
    RepeatStructure(RepeatStructureId),
    LyricLine(LyricLineId),
    ChordSymbol(ChordSymbolId),
    View(ViewId),
    /// Extension-defined object kind, identified by registry id plus the
    /// extension's own 128-bit identifier.
    Registered(ObjectKindRegistryId, u128),
}

impl TypedObjectId {
    /// The 16-bit discriminant for this variant (see the type docs).
    pub fn discriminant(&self) -> u16 {
        match self {
            TypedObjectId::Event(_) => 0,
            TypedObjectId::Pitch(_) => 1,
            TypedObjectId::Voice(_) => 2,
            TypedObjectId::Staff(_) => 3,
            TypedObjectId::StaffInstance(_) => 4,
            TypedObjectId::StaffGroup(_) => 5,
            TypedObjectId::Region(_) => 6,
            TypedObjectId::Instrument(_) => 7,
            TypedObjectId::PartDefinition(_) => 8,
            TypedObjectId::Measure(_) => 9,
            TypedObjectId::BarlineAlignmentGroup(_) => 10,
            TypedObjectId::Slur(_) => 11,
            TypedObjectId::Tie(_) => 12,
            TypedObjectId::Beam(_) => 13,
            TypedObjectId::Spanner(_) => 14,
            TypedObjectId::Marker(_) => 15,
            TypedObjectId::AnalyticalAnnotation(_) => 16,
            TypedObjectId::Comment(_) => 17,
            TypedObjectId::GraphicObject(_) => 18,
            TypedObjectId::GraphicGesture(_) => 19,
            TypedObjectId::TimeSignature(_) => 20,
            TypedObjectId::AnalysisLayer(_) => 21,
            TypedObjectId::Tuplet(_) => 22,
            TypedObjectId::RepeatStructure(_) => 23,
            TypedObjectId::LyricLine(_) => 24,
            TypedObjectId::ChordSymbol(_) => 25,
            TypedObjectId::View(_) => 26,
            TypedObjectId::Registered(..) => 27,
        }
    }

    /// The underlying 128-bit payload, ignoring the variant tag. For
    /// [`TypedObjectId::Registered`] this is the extension's own identifier
    /// (the registry id is encoded separately in the canonical bytes).
    fn payload_u128(&self) -> u128 {
        match self {
            TypedObjectId::Event(i) => i.as_u128(),
            TypedObjectId::Pitch(i) => i.as_u128(),
            TypedObjectId::Voice(i) => i.as_u128(),
            TypedObjectId::Staff(i) => i.as_u128(),
            TypedObjectId::StaffInstance(i) => i.as_u128(),
            TypedObjectId::StaffGroup(i) => i.as_u128(),
            TypedObjectId::Region(i) => i.as_u128(),
            TypedObjectId::Instrument(i) => i.as_u128(),
            TypedObjectId::PartDefinition(i) => i.as_u128(),
            TypedObjectId::Measure(i) => i.as_u128(),
            TypedObjectId::BarlineAlignmentGroup(i) => i.as_u128(),
            TypedObjectId::Slur(i) => i.as_u128(),
            TypedObjectId::Tie(i) => i.as_u128(),
            TypedObjectId::Beam(i) => i.as_u128(),
            TypedObjectId::Spanner(i) => i.as_u128(),
            TypedObjectId::Marker(i) => i.as_u128(),
            TypedObjectId::AnalyticalAnnotation(i) => i.as_u128(),
            TypedObjectId::Comment(i) => i.as_u128(),
            TypedObjectId::GraphicObject(i) => i.as_u128(),
            TypedObjectId::GraphicGesture(i) => i.as_u128(),
            TypedObjectId::TimeSignature(i) => i.as_u128(),
            TypedObjectId::AnalysisLayer(i) => i.as_u128(),
            TypedObjectId::Tuplet(i) => i.as_u128(),
            TypedObjectId::RepeatStructure(i) => i.as_u128(),
            TypedObjectId::LyricLine(i) => i.as_u128(),
            TypedObjectId::ChordSymbol(i) => i.as_u128(),
            TypedObjectId::View(i) => i.as_u128(),
            TypedObjectId::Registered(_, raw) => *raw,
        }
    }

    /// Canonical byte form for hashing, ordering, and equality (Chapter 5
    /// `TypedObjectId::canonical_bytes`): the 16-bit big-endian discriminant
    /// followed by the variant payload's canonical bytes.
    ///
    /// For [`TypedObjectId::Registered`] the payload is the registry id's
    /// 16 canonical bytes followed by the extension's own 16 `u128` bytes, so
    /// the form stays fixed-width and unambiguously decodable.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(2 + 32);
        out.extend_from_slice(&self.discriminant().to_be_bytes());
        match self {
            TypedObjectId::Registered(reg, raw) => {
                out.extend_from_slice(&reg.canonical_bytes());
                out.extend_from_slice(&raw.to_be_bytes());
            }
            other => out.extend_from_slice(&other.payload_u128().to_be_bytes()),
        }
        out
    }
}

impl PartialOrd for TypedObjectId {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for TypedObjectId {
    #[inline]
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        // The canonical byte form is the normative total order (Appendix D);
        // comparing it keeps `Ord` consistent with hashing and equality.
        self.canonical_bytes().cmp(&other.canonical_bytes())
    }
}

impl CanonicalEncode for TypedObjectId {
    #[inline]
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.canonical_bytes());
    }
}
impl CanonicalByteOrder for TypedObjectId {}

impl CanonicalDecode for TypedObjectId {
    fn decode_canonical(bytes: &[u8]) -> Result<Self, DecodeError> {
        if bytes.len() < 2 {
            return Err(DecodeError::UnexpectedLength {
                expected: 18,
                actual: bytes.len(),
            });
        }
        let disc = u16::from_be_bytes([bytes[0], bytes[1]]);
        let rest = &bytes[2..];
        // Helper: read exactly one 16-byte id payload.
        let one_id = |rest: &[u8]| -> Result<u128, DecodeError> {
            let arr: [u8; 16] = rest.try_into().map_err(|_| DecodeError::UnexpectedLength {
                expected: 16,
                actual: rest.len(),
            })?;
            Ok(u128::from_be_bytes(arr))
        };
        Ok(match disc {
            0 => TypedObjectId::Event(EventId::from_raw(one_id(rest)?)),
            1 => TypedObjectId::Pitch(PitchId::from_raw(one_id(rest)?)),
            2 => TypedObjectId::Voice(VoiceId::from_raw(one_id(rest)?)),
            3 => TypedObjectId::Staff(StaffId::from_raw(one_id(rest)?)),
            4 => TypedObjectId::StaffInstance(StaffInstanceId::from_raw(one_id(rest)?)),
            5 => TypedObjectId::StaffGroup(StaffGroupId::from_raw(one_id(rest)?)),
            6 => TypedObjectId::Region(RegionId::from_raw(one_id(rest)?)),
            7 => TypedObjectId::Instrument(InstrumentId::from_raw(one_id(rest)?)),
            8 => TypedObjectId::PartDefinition(PartDefinitionId::from_raw(one_id(rest)?)),
            9 => TypedObjectId::Measure(MeasureId::from_raw(one_id(rest)?)),
            10 => TypedObjectId::BarlineAlignmentGroup(BarlineAlignmentGroupId::from_raw(one_id(
                rest,
            )?)),
            11 => TypedObjectId::Slur(SlurId::from_raw(one_id(rest)?)),
            12 => TypedObjectId::Tie(TieId::from_raw(one_id(rest)?)),
            13 => TypedObjectId::Beam(BeamId::from_raw(one_id(rest)?)),
            14 => TypedObjectId::Spanner(SpannerId::from_raw(one_id(rest)?)),
            15 => TypedObjectId::Marker(MarkerId::from_raw(one_id(rest)?)),
            16 => {
                TypedObjectId::AnalyticalAnnotation(AnalyticalAnnotationId::from_raw(one_id(rest)?))
            }
            17 => TypedObjectId::Comment(CommentId::from_raw(one_id(rest)?)),
            18 => TypedObjectId::GraphicObject(GraphicObjectId::from_raw(one_id(rest)?)),
            19 => TypedObjectId::GraphicGesture(GraphicGestureId::from_raw(one_id(rest)?)),
            20 => TypedObjectId::TimeSignature(TimeSignatureId::from_raw(one_id(rest)?)),
            21 => TypedObjectId::AnalysisLayer(AnalysisLayerId::from_raw(one_id(rest)?)),
            22 => TypedObjectId::Tuplet(TupletId::from_raw(one_id(rest)?)),
            23 => TypedObjectId::RepeatStructure(RepeatStructureId::from_raw(one_id(rest)?)),
            24 => TypedObjectId::LyricLine(LyricLineId::from_raw(one_id(rest)?)),
            25 => TypedObjectId::ChordSymbol(ChordSymbolId::from_raw(one_id(rest)?)),
            26 => TypedObjectId::View(ViewId::from_raw(one_id(rest)?)),
            27 => {
                let arr: [u8; 32] = rest.try_into().map_err(|_| DecodeError::UnexpectedLength {
                    expected: 32,
                    actual: rest.len(),
                })?;
                let mut reg = [0u8; 16];
                let mut raw = [0u8; 16];
                reg.copy_from_slice(&arr[0..16]);
                raw.copy_from_slice(&arr[16..32]);
                TypedObjectId::Registered(
                    ObjectKindRegistryId::from_raw(u128::from_be_bytes(reg)),
                    u128::from_be_bytes(raw),
                )
            }
            _ => return Err(DecodeError::MalformedDomainTag),
        })
    }
}

/// The replica identifier plus identifier-generation state of a score
/// (Chapter 5 `IdentityContext`). A single monotonic counter suffices for all
/// identifier kinds.
#[derive(Clone, Debug)]
pub struct IdentityContext {
    /// This replica's identifier, generated at score creation.
    pub replica_id: ReplicaId,
    /// Monotonic counter for new identifiers; never reused, even after
    /// deletion (Chapter 5 §"Identifier Generation").
    pub next_counter: u64,
}

impl IdentityContext {
    /// Starts a fresh identity context for a replica, with the counter at 0.
    ///
    /// The caller is responsible for passing a non-reserved replica; a
    /// `debug_assert` guards against the reserved [`ReplicaId::SYSTEM_DERIVED`]
    /// namespace (Chapter 5: user-authored replicas must not use it), and the
    /// score-level invariant check ([`crate::check_invariants`], invariant 11)
    /// enforces it in every build profile. Use [`IdentityContext::try_new`] for
    /// a checked constructor.
    #[inline]
    pub fn new(replica_id: ReplicaId) -> Self {
        debug_assert!(
            !replica_id.is_system_derived(),
            "user-authored IdentityContext must not use the reserved SYSTEM_DERIVED replica"
        );
        IdentityContext {
            replica_id,
            next_counter: 0,
        }
    }

    /// Checked constructor: returns `None` for the reserved
    /// [`ReplicaId::SYSTEM_DERIVED`] namespace (Chapter 5 §"System-Derived
    /// Identifier Namespace").
    #[inline]
    pub fn try_new(replica_id: ReplicaId) -> Option<Self> {
        if replica_id.is_system_derived() {
            None
        } else {
            Some(IdentityContext {
                replica_id,
                next_counter: 0,
            })
        }
    }

    /// Starts a fresh identity context with a freshly generated replica id
    /// (QUICKSTART decision 1). [`ReplicaId::generate`] never yields the
    /// reserved namespace.
    pub fn fresh() -> Self {
        Self::new(ReplicaId::generate())
    }

    /// The next counter value, faulting loudly on the impossible `u64` wrap so
    /// a counter is never reused (Chapter 5 §"Identifier Generation"). Uses
    /// `checked_add` so the guarantee holds in *every* build profile, not only
    /// where `overflow-checks` is on.
    #[inline]
    fn take_counter(&mut self) -> u64 {
        let counter = self.next_counter;
        self.next_counter = self
            .next_counter
            .checked_add(1)
            .expect("identifier counter overflowed u64; counters must never be reused");
        counter
    }

    /// Mints the next identifier of any [`GraphId`] kind from this replica,
    /// advancing the monotonic counter (never reused).
    #[inline]
    pub fn mint<T: GraphId>(&mut self) -> T {
        let counter = self.take_counter();
        T::from_parts(self.replica_id, counter)
    }

    /// Mints the next [`OperationId`] from this replica.
    #[inline]
    pub fn mint_operation(&mut self) -> OperationId {
        let counter = self.take_counter();
        OperationId::new(self.replica_id, counter)
    }
}

/// Derives a system identifier of kind `T` in the [`ReplicaId::SYSTEM_DERIVED`]
/// namespace: its counter is `trunc64(BLAKE3(domain || canonical_inputs))`
/// (Chapter 5 §"System-Derived Identifiers"). The `domain` is a
/// [`SystemDomainTag`], so only a `MUSCS…` tag can seed a system identifier —
/// the precondition is enforced by the type, in every build profile.
///
/// Two replicas reducing identical canonical inputs derive byte-identical
/// system identifiers, which is the determinism the CRDT layer relies on.
#[inline]
pub fn derive_system_id<T: GraphId>(domain: SystemDomainTag, canonical_inputs: &[u8]) -> T {
    let counter = derive_system_counter(domain, canonical_inputs);
    T::from_parts(ReplicaId::SYSTEM_DERIVED, counter)
}

#[cfg(test)]
mod tests {
    use super::*;
    use epiphany_determinism::sorted_canonical;

    #[test]
    fn canonical_bytes_are_replica_then_counter_big_endian() {
        let id = EventId::new(ReplicaId(0x0102_0304_0506_0708), 0x1112_1314_1516_1718);
        assert_eq!(
            id.canonical_bytes(),
            [
                0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, // replica, BE
                0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, // counter, BE
            ]
        );
        assert_eq!(id.replica(), ReplicaId(0x0102_0304_0506_0708));
        assert_eq!(id.counter(), 0x1112_1314_1516_1718);
    }

    #[test]
    fn numeric_ord_matches_canonical_byte_order() {
        // Replica dominates the counter, exactly as big-endian bytes sort.
        let lo = VoiceId::new(ReplicaId(1), u64::MAX);
        let hi = VoiceId::new(ReplicaId(2), 0);
        assert!(lo < hi);
        assert!(lo.canonical_bytes() < hi.canonical_bytes());

        let mut ids = vec![
            VoiceId::new(ReplicaId(2), 0),
            VoiceId::new(ReplicaId(1), 5),
            VoiceId::new(ReplicaId(1), 0),
        ];
        ids.sort();
        let by_bytes = sorted_canonical(ids.clone());
        assert_eq!(ids, by_bytes);
    }

    #[test]
    fn cross_kind_ids_are_distinct_types_same_bytes() {
        // Same replica/counter, different kinds: identical canonical *id*
        // bytes, but a TypedObjectId tags them apart.
        let r = ReplicaId(7);
        let e = EventId::new(r, 3);
        let v = VoiceId::new(r, 3);
        assert_eq!(e.canonical_bytes(), v.canonical_bytes());
        let te = TypedObjectId::Event(e);
        let tv = TypedObjectId::Voice(v);
        assert_ne!(te, tv);
        assert_ne!(te.canonical_bytes(), tv.canonical_bytes());
    }

    #[test]
    fn replica_generation_never_yields_reserved() {
        assert!(ReplicaId::from_entropy([0xff; 8]).is_none());
        assert_eq!(ReplicaId::from_entropy([0; 8]), Some(ReplicaId(0)));
        for _ in 0..10_000 {
            assert!(!ReplicaId::generate().is_system_derived());
        }
    }

    #[test]
    fn mint_is_monotonic_and_per_replica() {
        let mut ctx = IdentityContext::new(ReplicaId(42));
        let a: EventId = ctx.mint();
        let b: VoiceId = ctx.mint();
        let c: EventId = ctx.mint();
        // One shared counter across kinds; strictly increasing; replica fixed.
        assert_eq!(a.counter(), 0);
        assert_eq!(b.counter(), 1);
        assert_eq!(c.counter(), 2);
        assert_eq!(a.replica(), ReplicaId(42));
        assert_eq!(b.replica(), ReplicaId(42));
    }

    #[test]
    fn system_derived_ids_are_deterministic_and_namespaced() {
        let a: VoiceId = derive_system_id(SystemDomainTag::VOICE, b"abc");
        let b: VoiceId = derive_system_id(SystemDomainTag::VOICE, b"abc");
        let c: VoiceId = derive_system_id(SystemDomainTag::VOICE, b"abd");
        assert_eq!(a, b, "identical inputs derive identical ids");
        assert_ne!(a, c, "different inputs derive different ids");
        assert_eq!(a.replica(), ReplicaId::SYSTEM_DERIVED);
        assert_eq!(
            a.counter(),
            derive_system_counter(SystemDomainTag::VOICE, b"abc")
        );
    }

    #[test]
    fn typed_object_id_round_trips_every_variant() {
        let r = ReplicaId(9);
        let cases = [
            TypedObjectId::Event(EventId::new(r, 1)),
            TypedObjectId::Pitch(PitchId::new(r, 2)),
            TypedObjectId::Voice(VoiceId::new(r, 3)),
            TypedObjectId::TimeSignature(TimeSignatureId::new(r, 20)),
            TypedObjectId::AnalysisLayer(AnalysisLayerId::new(r, 21)),
            TypedObjectId::Tuplet(TupletId::new(r, 22)),
            TypedObjectId::RepeatStructure(RepeatStructureId::new(r, 23)),
            TypedObjectId::LyricLine(LyricLineId::new(r, 24)),
            TypedObjectId::ChordSymbol(ChordSymbolId::new(r, 25)),
            TypedObjectId::View(ViewId::new(r, 26)),
            TypedObjectId::Registered(ObjectKindRegistryId::new(r, 99), 0xdead_beef),
        ];
        for c in cases {
            let bytes = c.canonical_bytes();
            assert_eq!(TypedObjectId::decode_canonical(&bytes).unwrap(), c);
            // Re-encode is byte-identical.
            assert_eq!(c.to_canonical_bytes(), bytes);
        }
    }

    #[test]
    fn operation_id_orders_by_replica_then_counter() {
        let a = OperationId::new(ReplicaId(1), 9);
        let b = OperationId::new(ReplicaId(2), 0);
        assert!(a < b);
        assert_eq!(
            OperationId::decode_canonical(&a.to_canonical_bytes()).unwrap(),
            a
        );
    }

    #[test]
    fn try_new_rejects_the_reserved_replica() {
        assert!(IdentityContext::try_new(ReplicaId::SYSTEM_DERIVED).is_none());
        assert!(IdentityContext::try_new(ReplicaId(1)).is_some());
    }

    #[test]
    #[should_panic(expected = "counter overflowed")]
    fn counter_overflow_faults_loudly_in_every_profile() {
        // `checked_add` guarantees this regardless of the build's overflow-checks
        // setting, so a counter is never silently reused.
        let mut ctx = IdentityContext::new(ReplicaId(1));
        ctx.next_counter = u64::MAX;
        let _: EventId = ctx.mint();
    }
}
