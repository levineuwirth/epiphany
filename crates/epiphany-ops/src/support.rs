//! Support types for the Chapter 6 surface: the extension-facing registry
//! identifiers, the [`AuthorId`], the [`ObjectKind`] discriminator, and the
//! [`SerializedCanonicalInputs`] blob.
//!
//! The registry identifiers are the spec's escape hatches: every Chapter 6
//! enum that the core vocabulary does not close ends in a `Registered(...Id)`
//! variant so a versioned extension registry can introduce its own values
//! without a spec revision (Chapter 11 §"Extension Registry Contract"). The
//! core does not interpret them; it only stores, orders, and hashes them, so a
//! plain 128-bit opaque newtype is the whole contract. v0 loads no external
//! registries (QUICKSTART "Don't … implement extension registries"); these
//! types exist so the canonical forms that *mention* extensions stay total.

use epiphany_determinism::{CanonicalByteOrder, CanonicalEncode};

/// Defines an opaque 128-bit registry/extension identifier newtype with the
/// canonical 16-byte big-endian form and byte-lexicographic order shared by
/// every identifier in the format (Appendix D §"Ordered Iteration").
macro_rules! registry_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Debug)]
        pub struct $name(pub u128);

        impl $name {
            /// Wraps a raw `u128`.
            #[inline]
            pub const fn from_raw(raw: u128) -> Self {
                $name(raw)
            }

            /// The raw `u128`.
            #[inline]
            pub const fn as_u128(self) -> u128 {
                self.0
            }

            /// The canonical 16-byte big-endian form.
            #[inline]
            pub const fn canonical_bytes(self) -> [u8; 16] {
                self.0.to_be_bytes()
            }
        }

        impl CanonicalEncode for $name {
            #[inline]
            fn encode_canonical(&self, out: &mut Vec<u8>) {
                out.extend_from_slice(&self.canonical_bytes());
            }
        }
        impl CanonicalByteOrder for $name {}
    };
}

registry_id!(
    /// Identifies an extension-defined primitive operation kind
    /// ([`crate::OperationKind::Registered`]).
    OperationKindRegistryId
);
registry_id!(
    /// Identifies an extension-defined conflict kind
    /// ([`crate::ConflictKind::ExtensionConflict`]).
    ConflictKindRegistryId
);
registry_id!(
    /// Identifies an extension-defined conflict-resolution action
    /// ([`crate::ResolutionAction::Registered`]).
    ResolutionRegistryId
);
registry_id!(
    /// Identifies an extension-defined repair kind
    /// ([`crate::RepairKind::Registered`]).
    RepairKindRegistryId
);
registry_id!(
    /// Identifies an extension-defined re-anchor reason
    /// ([`crate::ReanchorReason::DeclaredByExtension`]).
    ReanchorReasonRegistryId
);
registry_id!(
    /// Identifies an extension- or transport-defined replica-anomaly reason
    /// ([`crate::ReplicaAnomalyReason::Registered`]).
    ReplicaAnomalyRegistryId
);
registry_id!(
    /// Identifies an extension- or transport-defined integrity anomaly
    /// ([`crate::IntegrityAnomalyKind::Registered`]).
    IntegrityAnomalyRegistryId
);
registry_id!(
    /// Identifies an extension-declared precondition family; the extension's
    /// own catalog enumerates the specific codes
    /// ([`crate::PreconditionFailureReason::ExtensionPrecondition`]).
    ExtensionPreconditionId
);
registry_id!(
    /// Identifies a registered precondition-failure code from a versioned
    /// registry ([`crate::PreconditionFailureReason::Registered`]).
    PreconditionFailureRegistryId
);

/// The author of an operation (Chapter 6 §"Operation Envelopes"): "may differ
/// from replica in shared authoring sessions where multiple authors share a
/// replica." Modeled as an opaque 128-bit identifier; the core does not derive
/// behavior from it, so it is provenance metadata, not part of operation
/// identity (which is the [`epiphany_core::OperationId`]).
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Debug)]
pub struct AuthorId(pub u128);

impl AuthorId {
    /// Wraps a raw `u128`.
    #[inline]
    pub const fn from_raw(raw: u128) -> Self {
        AuthorId(raw)
    }

    /// The canonical 16-byte big-endian form.
    #[inline]
    pub const fn canonical_bytes(self) -> [u8; 16] {
        self.0.to_be_bytes()
    }
}

impl CanonicalEncode for AuthorId {
    #[inline]
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.canonical_bytes());
    }
}
impl CanonicalByteOrder for AuthorId {}

/// The kind of object a *system-derived* identifier names, used by
/// [`crate::IntegrityAnomalyKind::SystemIdentifierCollision`] to disambiguate
/// a collided 64-bit counter within its kind (Chapter 5 §"System-Derived
/// Counter Collisions": the collision is checked "within the same typed
/// identifier kind").
///
/// Only the kinds that the spec actually derives into the
/// [`epiphany_core::ReplicaId::SYSTEM_DERIVED`] namespace are enumerated —
/// system-promoted [`Voice`](ObjectKind::Voice)s and content-derived synthetic
/// [`Pitch`](ObjectKind::Pitch)es (Chapter 5) — plus a [`Registered`](ObjectKind::Registered)
/// escape for extension-introduced system kinds. A non-system object kind
/// cannot collide in this namespace, so it is intentionally not representable
/// here.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum ObjectKind {
    /// A system-promoted voice (`MUSCSVCE`).
    Voice,
    /// A content-derived synthetic pitch (`MUSCSPCH`).
    Pitch,
    /// An extension-introduced system-derived object kind.
    Registered(crate::support::OperationKindRegistryId),
}

impl ObjectKind {
    /// The discriminant byte for this kind. Part of the canonical bytes that
    /// feed an anomaly's identity, so it is fixed here.
    #[inline]
    pub fn discriminant(&self) -> u8 {
        match self {
            ObjectKind::Voice => 0,
            ObjectKind::Pitch => 1,
            ObjectKind::Registered(_) => 2,
        }
    }
}

impl CanonicalEncode for ObjectKind {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        out.push(self.discriminant());
        if let ObjectKind::Registered(reg) = self {
            reg.encode_canonical(out);
        }
    }
}

/// The canonical input bytes that were hashed to derive a system counter
/// (Chapter 5 §"System-Derived Counter Collisions": the two colliding input
/// sets are retained so recovery can tell them apart). Opaque to the core.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct SerializedCanonicalInputs(pub Vec<u8>);

impl CanonicalEncode for SerializedCanonicalInputs {
    #[inline]
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        crate::encode::push_lp_bytes(out, &self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use epiphany_determinism::sort_canonical;

    #[test]
    fn registry_ids_order_by_canonical_bytes() {
        let mut v = vec![
            OperationKindRegistryId(3),
            OperationKindRegistryId(1),
            OperationKindRegistryId(2),
        ];
        sort_canonical(&mut v);
        assert_eq!(
            v,
            vec![
                OperationKindRegistryId(1),
                OperationKindRegistryId(2),
                OperationKindRegistryId(3),
            ]
        );
    }

    #[test]
    fn object_kind_discriminant_separates_voice_and_pitch() {
        let mut a = Vec::new();
        ObjectKind::Voice.encode_canonical(&mut a);
        let mut b = Vec::new();
        ObjectKind::Pitch.encode_canonical(&mut b);
        assert_ne!(a, b);
    }
}
