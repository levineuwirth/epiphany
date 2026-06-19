//! Floating-point hygiene for canonical state.
//!
//! Appendix D §"Floating-Point Values in Canonical State":
//!
//! * Canonical stored `f64` values **must** be finite IEEE 754 binary64.
//!   NaN and infinity must never appear in canonical chunks.
//! * `-0.0` **must** be canonicalized to `+0.0` before storage; two values
//!   that differ only in zero sign are canonically equal.
//! * Canonical `f64` values are serialized as little-endian IEEE 754 octets
//!   *after* the `-0.0 -> +0.0` canonicalization, and canonical equality is
//!   byte equality of that representation.
//!
//! Floating point is admissible only in advisory/acoustic/tuning/tempo/
//! layout/quality contexts. Identity (ids, ordering, membership, hashes)
//! never uses it.

use core::cmp::Ordering;
use core::hash::{Hash, Hasher};

/// Maps `-0.0` to `+0.0`, leaving every other value (including NaN and
/// infinities) unchanged. This is the `-0.0 -> +0.0` canonicalization the
/// spec requires before any canonical comparison or serialization.
///
/// Implemented with an equality test rather than `x + 0.0` so it is immune to
/// the ambient rounding mode (Appendix D §"Rounding and CPU Behavior").
#[inline]
pub fn canonicalize_zero(x: f64) -> f64 {
    // `-0.0 == 0.0` is true in IEEE 754, so this branch catches both zeros
    // and yields the canonical `+0.0`; all non-zero values pass through.
    if x == 0.0 {
        0.0
    } else {
        x
    }
}

/// Debug-only guard that a value is *already* in canonical form: finite, not
/// NaN/inf, and not negative zero. Use it to assert an invariant on a value
/// you believe a prior step has already canonicalized — not on a raw value
/// you are about to canonicalize (for that, the canonicalizing functions
/// accept `-0.0` and fix it). In release builds this compiles away; the hard
/// rejection of NaN/inf happens at the typed boundary ([`CanonicalF64`]) and
/// at decode time, not on every arithmetic step.
#[inline]
#[track_caller]
pub fn debug_assert_canonical(x: f64) {
    debug_assert!(
        x.is_finite(),
        "non-finite f64 ({x:?}) in canonical state (Appendix D: NaN/inf forbidden)"
    );
    debug_assert!(
        !(x == 0.0 && x.is_sign_negative()),
        "-0.0 in canonical state without canonicalization (Appendix D: -0.0 -> +0.0)"
    );
}

/// The canonical 8-byte little-endian representation of a finite `f64`, or
/// `None` if `x` is NaN or infinite.
///
/// Rejection is at runtime in *all* build profiles: Appendix D §"Permitted
/// Forms" requires implementations to *reject* NaN/infinity at serialization
/// time, so a debug-only assertion would not suffice. `-0.0` is accepted and
/// canonicalized to `+0.0`. This is the fallible convenience form of
/// [`CanonicalF64::new`] followed by [`CanonicalF64::to_le_bytes`].
#[inline]
pub fn canonical_f64_bytes(x: f64) -> Option<[u8; 8]> {
    CanonicalF64::new(x).map(|c| c.to_le_bytes())
}

/// A finite `f64` in canonical form: never NaN, never infinite, never `-0.0`.
///
/// Construction is the only way to get one, and it enforces the invariants, so
/// every `CanonicalF64` is admissible in canonical state by construction.
/// Equality and hashing are defined over the canonical serialized bytes (so
/// `+0.0 == -0.0`'s canonicalization is already absorbed); ordering is IEEE
/// 754 ordered comparison, which is total here because the value is finite.
#[derive(Copy, Clone, Debug, Default)]
pub struct CanonicalF64(f64);

impl CanonicalF64 {
    /// Wraps `x` if it is finite, applying `-0.0 -> +0.0`. Returns `None` for
    /// NaN or infinity (which the spec forbids from canonical state).
    #[inline]
    pub fn new(x: f64) -> Option<Self> {
        if x.is_finite() {
            Some(CanonicalF64(canonicalize_zero(x)))
        } else {
            None
        }
    }

    /// The wrapped finite value (with `+0.0` for any zero).
    #[inline]
    pub fn get(self) -> f64 {
        self.0
    }

    /// Canonical little-endian serialization (8 bytes). Infallible: the
    /// wrapped value is finite and `+0.0`-normalized by construction.
    #[inline]
    pub fn to_le_bytes(self) -> [u8; 8] {
        self.0.to_le_bytes()
    }

    /// Decodes canonical little-endian bytes, rejecting NaN/inf and
    /// canonicalizing `-0.0`. Returns `None` on a non-finite payload, which
    /// the spec instructs readers to treat as data corruption.
    #[inline]
    pub fn from_le_bytes(bytes: [u8; 8]) -> Option<Self> {
        Self::new(f64::from_le_bytes(bytes))
    }
}

impl PartialEq for CanonicalF64 {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        // Byte equality of the canonical representation (Appendix D §Equality).
        self.0.to_le_bytes() == other.0.to_le_bytes()
    }
}

impl Eq for CanonicalF64 {}

impl Hash for CanonicalF64 {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.to_le_bytes().hash(state);
    }
}

impl PartialOrd for CanonicalF64 {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CanonicalF64 {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        // Finite + zero-canonicalized, so `total_cmp` agrees with numeric
        // order and is consistent with the byte-equality `Eq` above.
        self.0.total_cmp(&other.0)
    }
}

impl TryFrom<f64> for CanonicalF64 {
    type Error = NonFiniteError;

    #[inline]
    fn try_from(x: f64) -> Result<Self, Self::Error> {
        Self::new(x).ok_or(NonFiniteError)
    }
}

/// Returned when a non-finite `f64` is offered where canonical state requires
/// a finite value.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct NonFiniteError;

impl core::fmt::Display for NonFiniteError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("non-finite f64 is inadmissible in canonical state")
    }
}

impl std::error::Error for NonFiniteError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_zero_canonicalizes_to_positive_zero() {
        assert_eq!(canonicalize_zero(-0.0).to_bits(), 0.0f64.to_bits());
        assert_eq!(canonical_f64_bytes(-0.0), Some(0.0f64.to_le_bytes()));
        assert_eq!(canonical_f64_bytes(f64::NAN), None);
        assert_eq!(canonical_f64_bytes(f64::INFINITY), None);

        let neg = CanonicalF64::new(-0.0).unwrap();
        let pos = CanonicalF64::new(0.0).unwrap();
        assert_eq!(neg, pos);
        assert_eq!(neg.to_le_bytes(), pos.to_le_bytes());
        assert_eq!(neg.get().to_bits(), 0.0f64.to_bits()); // truly +0.0
    }

    #[test]
    fn non_finite_is_rejected() {
        assert!(CanonicalF64::new(f64::NAN).is_none());
        assert!(CanonicalF64::new(f64::INFINITY).is_none());
        assert!(CanonicalF64::new(f64::NEG_INFINITY).is_none());
        assert!(CanonicalF64::try_from(f64::NAN).is_err());
        assert!(CanonicalF64::from_le_bytes(f64::NAN.to_le_bytes()).is_none());
    }

    #[test]
    fn ordering_is_numeric_and_consistent_with_equality() {
        let a = CanonicalF64::new(-1.5).unwrap();
        let b = CanonicalF64::new(0.0).unwrap();
        let c = CanonicalF64::new(2.25).unwrap();
        assert!(a < b && b < c);
        assert_eq!(b.cmp(&CanonicalF64::new(-0.0).unwrap()), Ordering::Equal);
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "-0.0 in canonical state")]
    fn debug_assert_canonical_rejects_negative_zero() {
        debug_assert_canonical(-0.0);
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "NaN/inf forbidden")]
    fn debug_assert_canonical_rejects_nan() {
        debug_assert_canonical(f64::NAN);
    }

    #[test]
    fn ordinary_values_round_trip_bytes() {
        for v in [1.0, -1.0, 123.456, -2.5, 1e300, -1e-300, f64::MIN, f64::MAX] {
            let c = CanonicalF64::new(v).unwrap();
            assert_eq!(CanonicalF64::from_le_bytes(c.to_le_bytes()).unwrap(), c);
        }
    }
}
