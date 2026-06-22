//! Time and duration primitives (Chapter 3).
//!
//! The core has **two clocks** (Chapter 3 §"Design Principles"): musical time,
//! measured in exact whole-note rationals ([`RationalTime`]), and wall-clock
//! time, measured in fixed-point nanoseconds ([`WallClockTime`]). Position and
//! duration are *distinct types* whose algebra is enforced at the type level
//! ([`MusicalPosition`] + [`MusicalDuration`] → [`MusicalPosition`];
//! position − position → duration; position + position is not defined).
//!
//! Exactness is non-negotiable: musical time is an exact rational, never a
//! float (Chapter 3; Appendix D §"Exact and Quantized Representations"). The
//! recommended inline-or-promoted representation packs the common case and
//! promotes to arbitrary precision on overflow, with no observable behavioural
//! difference (Chapter 3 §"Promotion and Demotion").

use core::num::NonZeroU32;
use core::ops::{Add, Sub};
use std::sync::Arc;

use epiphany_determinism::{CanonicalDecode, CanonicalEncode, DecodeError};
use num_bigint::{BigInt, Sign};
use num_rational::BigRational;
use num_traits::{Signed, Zero};

use crate::ids::{EventId, MeasureId, RegionId};

/// An exact rational musical-time value: a [`MusicalPosition`] or
/// [`MusicalDuration`] before the newtype distinction is applied (Chapter 3
/// §"The Rational Time Type"). The unit is the **whole note**: a quarter note
/// is `1/4`, a triplet eighth is `1/12`.
///
/// Representation is inline-or-promoted (Chapter 3 §"Recommended
/// Implementation"): the inline [`SmallRational`] (`i32` numerator,
/// `NonZeroU32` denominator) covers the overwhelmingly common case; arithmetic
/// that exceeds it silently promotes to an [`Arc`]-shared [`BigRational`].
///
/// **Canonical-form invariant.** A value is [`RationalTime::Small`] *if and
/// only if* its normalized numerator fits `i32` and its denominator fits a
/// nonzero `u32`. Every constructor and operation re-establishes this, so two
/// numerically-equal values always share a variant and demotion is never
/// observable (Chapter 3 §"Promotion and Demotion").
#[derive(Clone)]
pub enum RationalTime {
    /// Inline case: fits in 8 bytes, always normalized.
    Small(SmallRational),
    /// Promoted case: arbitrary-precision rational, used only when arithmetic
    /// overflows the inline range.
    Large(Arc<BigRational>),
}

/// The inline rational: `i32` numerator over a `NonZeroU32` denominator,
/// always normalized so `gcd(|numerator|, denominator) == 1` (Chapter 3).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct SmallRational {
    numerator: i32,
    denominator: NonZeroU32,
}

impl SmallRational {
    /// The numerator.
    #[inline]
    pub fn numerator(self) -> i32 {
        self.numerator
    }
    /// The (strictly positive) denominator.
    #[inline]
    pub fn denominator(self) -> u32 {
        self.denominator.get()
    }
}

impl RationalTime {
    /// The additive identity, `0/1`.
    pub fn zero() -> Self {
        RationalTime::Small(SmallRational {
            numerator: 0,
            denominator: NonZeroU32::new(1).unwrap(),
        })
    }

    /// The multiplicative identity and the duration of a whole note, `1/1`.
    pub fn one() -> Self {
        RationalTime::from_int(1)
    }

    /// An integer count of whole notes.
    pub fn from_int(n: i32) -> Self {
        RationalTime::Small(SmallRational {
            numerator: n,
            denominator: NonZeroU32::new(1).unwrap(),
        })
    }

    /// Constructs `numerator / denominator`, normalized. Returns `None` if the
    /// denominator is zero (the only non-representable input; magnitude is
    /// handled by promotion).
    pub fn new(numerator: i64, denominator: i64) -> Option<Self> {
        if denominator == 0 {
            return None;
        }
        Some(Self::from_big(BigRational::new(
            BigInt::from(numerator),
            BigInt::from(denominator),
        )))
    }

    /// Builds from a (reduced or unreduced) [`BigRational`], demoting to
    /// [`RationalTime::Small`] when the normalized value fits the inline range.
    /// This is the single chokepoint that maintains the canonical-form
    /// invariant.
    fn from_big(value: BigRational) -> Self {
        // `BigRational` keeps the denominator positive and the fraction
        // reduced, so the sign lives on the numerator.
        let numer = value.numer();
        let denom = value.denom();
        if let (Some(n), Some(d)) = (bigint_to_i32(numer), bigint_to_u32(denom)) {
            if let Some(d) = NonZeroU32::new(d) {
                return RationalTime::Small(SmallRational {
                    numerator: n,
                    denominator: d,
                });
            }
        }
        RationalTime::Large(Arc::new(value))
    }

    /// The value as a [`BigRational`] (allocates for the inline case; used on
    /// the slow arithmetic path and for canonical encoding).
    fn to_big(&self) -> BigRational {
        match self {
            RationalTime::Small(s) => {
                BigRational::new(BigInt::from(s.numerator), BigInt::from(s.denominator.get()))
            }
            RationalTime::Large(b) => (**b).clone(),
        }
    }

    /// Whether the value is exactly zero.
    pub fn is_zero(&self) -> bool {
        match self {
            RationalTime::Small(s) => s.numerator == 0,
            RationalTime::Large(b) => b.is_zero(),
        }
    }

    /// Whether the value is strictly negative.
    pub fn is_negative(&self) -> bool {
        match self {
            RationalTime::Small(s) => s.numerator < 0,
            RationalTime::Large(b) => b.is_negative(),
        }
    }

    /// A lossy `f64` approximation. For *advisory* use only — tempo conversion,
    /// spacing hints, diagnostics — never canonical state (musical time is the
    /// exact rational; Appendix D §"Exact and Quantized Representations").
    pub fn to_f64(&self) -> f64 {
        match self {
            RationalTime::Small(s) => s.numerator as f64 / s.denominator.get() as f64,
            RationalTime::Large(b) => {
                use num_traits::ToPrimitive;
                b.to_f64().unwrap_or(f64::NAN)
            }
        }
    }

    /// Exact addition.
    pub fn add(&self, other: &Self) -> Self {
        if let (RationalTime::Small(a), RationalTime::Small(b)) = (self, other) {
            // Fast path: a/b + c/d in widened i128, then fit-or-promote.
            let (n, d) = (a.numerator as i128, a.denominator.get() as i128);
            let (n2, d2) = (b.numerator as i128, b.denominator.get() as i128);
            if let Some(r) = small_from_i128(n * d2 + n2 * d, d * d2) {
                return r;
            }
        }
        RationalTime::from_big(self.to_big() + other.to_big())
    }

    /// Exact subtraction.
    pub fn sub(&self, other: &Self) -> Self {
        if let (RationalTime::Small(a), RationalTime::Small(b)) = (self, other) {
            let (n, d) = (a.numerator as i128, a.denominator.get() as i128);
            let (n2, d2) = (b.numerator as i128, b.denominator.get() as i128);
            if let Some(r) = small_from_i128(n * d2 - n2 * d, d * d2) {
                return r;
            }
        }
        RationalTime::from_big(self.to_big() - other.to_big())
    }

    /// Exact multiplication (used to scale durations by a tuplet ratio, etc.).
    pub fn mul(&self, other: &Self) -> Self {
        if let (RationalTime::Small(a), RationalTime::Small(b)) = (self, other) {
            let n = a.numerator as i128 * b.numerator as i128;
            let d = a.denominator.get() as i128 * b.denominator.get() as i128;
            if let Some(r) = small_from_i128(n, d) {
                return r;
            }
        }
        RationalTime::from_big(self.to_big() * other.to_big())
    }

    /// Sums a sequence of rationals exactly (left fold). Useful for the
    /// tuplet- and decomposition-sum invariants (Chapter 5).
    pub fn sum<'a, I: IntoIterator<Item = &'a RationalTime>>(iter: I) -> RationalTime {
        let mut acc = RationalTime::zero();
        for r in iter {
            acc = acc.add(r);
        }
        acc
    }
}

/// Reduces `num/den` (with arbitrary-sign `den`) and returns it as a
/// [`RationalTime::Small`] iff it fits the inline range; otherwise `None`,
/// signalling the caller to take the [`BigRational`] path.
fn small_from_i128(mut num: i128, mut den: i128) -> Option<RationalTime> {
    if den == 0 {
        return None;
    }
    if den < 0 {
        num = -num;
        den = -den;
    }
    let g = {
        let mut a = num.unsigned_abs();
        let mut b = den as u128;
        while b != 0 {
            let t = a % b;
            a = b;
            b = t;
        }
        a.max(1)
    } as i128;
    num /= g;
    den /= g;
    let n: i32 = i32::try_from(num).ok()?;
    let d: u32 = u32::try_from(den).ok()?;
    Some(RationalTime::Small(SmallRational {
        numerator: n,
        denominator: NonZeroU32::new(d)?,
    }))
}

fn bigint_to_i32(b: &BigInt) -> Option<i32> {
    i32::try_from(b.clone()).ok()
}
fn bigint_to_u32(b: &BigInt) -> Option<u32> {
    u32::try_from(b.clone()).ok()
}

impl PartialEq for RationalTime {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            // Canonical-form invariant: equal values share a variant, so the
            // fast inline comparison is exact for the common case.
            (RationalTime::Small(a), RationalTime::Small(b)) => a == b,
            _ => self.cmp(other) == core::cmp::Ordering::Equal,
        }
    }
}
impl Eq for RationalTime {}

impl Ord for RationalTime {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        if let (RationalTime::Small(a), RationalTime::Small(b)) = (self, other) {
            // Cross-multiply in i128 to avoid overflow (Chapter 3 §"Equality
            // and Ordering"). Denominators are positive, so the inequality
            // direction is preserved.
            let lhs = a.numerator as i128 * b.denominator.get() as i128;
            let rhs = b.numerator as i128 * a.denominator.get() as i128;
            return lhs.cmp(&rhs);
        }
        self.to_big().cmp(&other.to_big())
    }
}
impl PartialOrd for RationalTime {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl core::hash::Hash for RationalTime {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        // Within each variant the normalized form is unique, and the
        // canonical-form invariant guarantees numerically-equal values never
        // straddle the variant boundary, so per-variant hashing is consistent
        // with `Eq`.
        match self {
            RationalTime::Small(s) => {
                state.write_u8(0);
                state.write_i32(s.numerator);
                state.write_u32(s.denominator.get());
            }
            RationalTime::Large(b) => {
                state.write_u8(1);
                let (sign, bytes) = b.numer().to_bytes_be();
                state.write_i8(match sign {
                    Sign::Minus => -1,
                    Sign::NoSign => 0,
                    Sign::Plus => 1,
                });
                state.write(&bytes);
                state.write(&b.denom().to_bytes_be().1);
            }
        }
    }
}

impl core::fmt::Debug for RationalTime {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            RationalTime::Small(s) => write!(f, "{}/{}", s.numerator, s.denominator.get()),
            RationalTime::Large(b) => write!(f, "{}/{} (large)", b.numer(), b.denom()),
        }
    }
}

impl Default for RationalTime {
    fn default() -> Self {
        RationalTime::zero()
    }
}

/// Canonical, arbitrary-precision, reversible byte form for a rational: the
/// numerator's sign and big-endian magnitude (length-prefixed), then the
/// positive denominator's big-endian magnitude (length-prefixed). The value is
/// always reduced first, so equal rationals encode to equal bytes (Appendix D
/// §"Canonical serialization determinism"). RATIFIED by Pass 11 (item 1.7,
/// P11-4): this primitive layout is now normative in core_spec §"Binary Format
/// Companion", Requirement `req:format:rationaltime-encoding`; the full
/// composite wire format remains the Binary Format companion's (Agent J), which
/// inherits the ratified convention baseline (`req:format:codec-conventions`).
impl CanonicalEncode for RationalTime {
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        let big = self.to_big();
        let (sign, numer_mag) = big.numer().to_bytes_be();
        let denom_mag = big.denom().to_bytes_be().1;
        out.push(match sign {
            Sign::Minus => 2,
            Sign::NoSign => 0,
            Sign::Plus => 1,
        });
        out.extend_from_slice(&(numer_mag.len() as u32).to_le_bytes());
        out.extend_from_slice(&numer_mag);
        out.extend_from_slice(&(denom_mag.len() as u32).to_le_bytes());
        out.extend_from_slice(&denom_mag);
    }
}
impl CanonicalDecode for RationalTime {
    fn decode_canonical(bytes: &[u8]) -> Result<Self, DecodeError> {
        let mut cur = bytes;
        let take = |cur: &mut &[u8], n: usize| -> Result<Vec<u8>, DecodeError> {
            if cur.len() < n {
                return Err(DecodeError::UnexpectedLength {
                    expected: n,
                    actual: cur.len(),
                });
            }
            let (head, tail) = cur.split_at(n);
            *cur = tail;
            Ok(head.to_vec())
        };
        let sign_byte = take(&mut cur, 1)?[0];
        let sign = match sign_byte {
            0 => Sign::NoSign,
            1 => Sign::Plus,
            2 => Sign::Minus,
            _ => return Err(DecodeError::MalformedDomainTag),
        };
        let numer_len = u32::from_le_bytes(take(&mut cur, 4)?.try_into().unwrap()) as usize;
        let numer_mag = take(&mut cur, numer_len)?;
        let denom_len = u32::from_le_bytes(take(&mut cur, 4)?.try_into().unwrap()) as usize;
        let denom_mag = take(&mut cur, denom_len)?;
        if !cur.is_empty() {
            return Err(DecodeError::UnexpectedLength {
                expected: bytes.len() - cur.len(),
                actual: bytes.len(),
            });
        }
        let numer = BigInt::from_bytes_be(sign, &numer_mag);
        let denom = BigInt::from_bytes_be(Sign::Plus, &denom_mag);
        if denom.is_zero() {
            return Err(DecodeError::MalformedDomainTag);
        }
        Ok(RationalTime::from_big(BigRational::new(numer, denom)))
    }
}

/// A point in musical time, relative to the origin of a time region
/// (Chapter 3 §"Position and Duration as Distinct Types"). Wraps a
/// [`RationalTime`]; adding two positions is intentionally not defined.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Default)]
pub struct MusicalPosition(pub RationalTime);

/// A span of musical time (Chapter 3). Wraps a [`RationalTime`].
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Default)]
pub struct MusicalDuration(pub RationalTime);

impl MusicalPosition {
    /// The region origin, `0`.
    pub fn origin() -> Self {
        MusicalPosition(RationalTime::zero())
    }
    /// The underlying rational.
    pub fn rational(&self) -> &RationalTime {
        &self.0
    }
}

impl MusicalDuration {
    /// The zero-length duration.
    pub fn zero() -> Self {
        MusicalDuration(RationalTime::zero())
    }
    /// A whole note (`1/1`).
    pub fn whole() -> Self {
        MusicalDuration(RationalTime::one())
    }
    /// The underlying rational.
    pub fn rational(&self) -> &RationalTime {
        &self.0
    }
    /// Whether the duration is strictly positive (the usual well-formedness
    /// condition for a sounding event).
    pub fn is_positive(&self) -> bool {
        !self.0.is_zero() && !self.0.is_negative()
    }
    /// Sums a sequence of durations exactly.
    pub fn sum<'a, I: IntoIterator<Item = &'a MusicalDuration>>(iter: I) -> MusicalDuration {
        MusicalDuration(RationalTime::sum(iter.into_iter().map(|d| &d.0)))
    }
}

// The type-level algebra of Chapter 3 §"Position and Duration as Distinct
// Types". `MusicalPosition + MusicalPosition` is deliberately absent.
impl Add<MusicalDuration> for MusicalPosition {
    type Output = MusicalPosition;
    fn add(self, rhs: MusicalDuration) -> MusicalPosition {
        MusicalPosition(self.0.add(&rhs.0))
    }
}
impl Add<MusicalDuration> for MusicalDuration {
    type Output = MusicalDuration;
    fn add(self, rhs: MusicalDuration) -> MusicalDuration {
        MusicalDuration(self.0.add(&rhs.0))
    }
}
impl Sub<MusicalPosition> for MusicalPosition {
    type Output = MusicalDuration;
    fn sub(self, rhs: MusicalPosition) -> MusicalDuration {
        MusicalDuration(self.0.sub(&rhs.0))
    }
}
impl Sub<MusicalDuration> for MusicalDuration {
    type Output = MusicalDuration;
    fn sub(self, rhs: MusicalDuration) -> MusicalDuration {
        MusicalDuration(self.0.sub(&rhs.0))
    }
}

macro_rules! delegate_canon {
    ($name:ident) => {
        impl CanonicalEncode for $name {
            #[inline]
            fn encode_canonical(&self, out: &mut Vec<u8>) {
                self.0.encode_canonical(out);
            }
        }
        impl CanonicalDecode for $name {
            #[inline]
            fn decode_canonical(bytes: &[u8]) -> Result<Self, DecodeError> {
                Ok($name(RationalTime::decode_canonical(bytes)?))
            }
        }
    };
}
delegate_canon!(MusicalPosition);
delegate_canon!(MusicalDuration);

/// A point in wall-clock time, in nanoseconds from a region origin (Chapter 3
/// §"Wall-Clock Time"). 64-bit signed: range ±~292 years. Floating-point
/// wall-clock time is forbidden in stored data.
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Default)]
pub struct WallClockTime(pub i64);

/// A span of wall-clock time, in nanoseconds.
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Default)]
pub struct WallClockDuration(pub i64);

impl WallClockTime {
    /// Canonical little-endian bytes (8, `i64`), matching the integer
    /// convention of [`epiphany_determinism::QuantizedCoord`].
    #[inline]
    pub fn to_le_bytes(self) -> [u8; 8] {
        self.0.to_le_bytes()
    }
}
impl CanonicalEncode for WallClockTime {
    #[inline]
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.0.to_le_bytes());
    }
}
impl CanonicalDecode for WallClockTime {
    #[inline]
    fn decode_canonical(bytes: &[u8]) -> Result<Self, DecodeError> {
        let arr: [u8; 8] = bytes
            .try_into()
            .map_err(|_| DecodeError::UnexpectedLength {
                expected: 8,
                actual: bytes.len(),
            })?;
        Ok(WallClockTime(i64::from_le_bytes(arr)))
    }
}
impl CanonicalEncode for WallClockDuration {
    #[inline]
    fn encode_canonical(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.0.to_le_bytes());
    }
}
impl CanonicalDecode for WallClockDuration {
    #[inline]
    fn decode_canonical(bytes: &[u8]) -> Result<Self, DecodeError> {
        let arr: [u8; 8] = bytes
            .try_into()
            .map_err(|_| DecodeError::UnexpectedLength {
                expected: 8,
                actual: bytes.len(),
            })?;
        Ok(WallClockDuration(i64::from_le_bytes(arr)))
    }
}

/// Which boundary of a measure an anchor points at.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum MeasurePosition {
    Start,
    End,
}

/// Which edge of a region an anchor points at.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum RegionEdge {
    Start,
    End,
}

/// An offset applied to an anchor target (Chapter 3 §"Time Anchors"). The
/// admissible variant is constrained by the target's enclosing region's time
/// model — see [`OffsetKind`] and invariant 9 in the `invariants` module.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum AnchorOffset {
    /// Offset in musical time. Valid for targets in metric regions (and
    /// musical-discipline aleatoric regions).
    Musical(MusicalDuration),
    /// Offset in wall-clock time. Valid for targets in proportional regions
    /// (and wall-clock-discipline aleatoric regions).
    WallClock(WallClockDuration),
    /// No offset; the anchor refers to the target's reference point exactly.
    /// Valid in any region.
    Zero,
}

/// The clock an [`AnchorOffset`] is expressed in, used to check it against a
/// region's time model (Chapter 3; invariant 9).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum OffsetKind {
    Musical,
    WallClock,
    Zero,
}

impl AnchorOffset {
    /// The clock this offset is expressed in.
    pub fn kind(&self) -> OffsetKind {
        match self {
            AnchorOffset::Musical(_) => OffsetKind::Musical,
            AnchorOffset::WallClock(_) => OffsetKind::WallClock,
            AnchorOffset::Zero => OffsetKind::Zero,
        }
    }
}

/// A stored reference to a point in time (Chapter 3 §"Time Anchors"). Stored
/// references to *external* time points must anchor to identified objects plus
/// offsets, never to absolute positions that could shift under edits.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum TimeAnchor {
    /// Anchored to a specific event. Survives edits that do not delete it.
    Event { id: EventId, offset: AnchorOffset },
    /// Anchored to a measure boundary. Survives measure reordering.
    Measure {
        id: MeasureId,
        position: MeasurePosition,
        offset: AnchorOffset,
    },
    /// Anchored to the start or end of a region.
    Region {
        id: RegionId,
        edge: RegionEdge,
        offset: AnchorOffset,
    },
    /// Anchored to absolute wall-clock time. Used for film and audio sync.
    WallClock { time: WallClockTime },
}

impl TimeAnchor {
    /// The offset of this anchor, if it has one (a [`TimeAnchor::WallClock`]
    /// anchor carries no separate offset; its position is absolute).
    pub fn offset(&self) -> Option<&AnchorOffset> {
        match self {
            TimeAnchor::Event { offset, .. }
            | TimeAnchor::Measure { offset, .. }
            | TimeAnchor::Region { offset, .. } => Some(offset),
            TimeAnchor::WallClock { .. } => None,
        }
    }
}

/// An event's position within its owning voice and region (Chapter 5
/// §"Event Position and Duration"). Unioned over the two clocks; the admissible
/// variant is fixed by the enclosing region's time model.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum EventPosition {
    Musical(MusicalPosition),
    WallClock(WallClockTime),
}

/// An event's duration (Chapter 5). Unioned over musical, wall-clock, and
/// indeterminate forms.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum EventDuration {
    Musical(MusicalDuration),
    WallClock(WallClockDuration),
    Indeterminate(DurationBounds),
}

/// A concrete (non-indeterminate) duration in one of the two clocks
/// (Chapter 5). The bounds of an indeterminate duration are concrete, which
/// prevents recursive indeterminacy in the type system.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum ConcreteDuration {
    Musical(MusicalDuration),
    WallClock(WallClockDuration),
}

/// The clock a position/duration coordinate is expressed in, used to check it
/// against a region's time model (Chapter 5; invariant 4).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum CoordinateKind {
    Musical,
    WallClock,
}

impl EventPosition {
    /// The clock this position is expressed in.
    pub fn kind(&self) -> CoordinateKind {
        match self {
            EventPosition::Musical(_) => CoordinateKind::Musical,
            EventPosition::WallClock(_) => CoordinateKind::WallClock,
        }
    }
}

impl ConcreteDuration {
    /// The clock this duration is expressed in.
    pub fn kind(&self) -> CoordinateKind {
        match self {
            ConcreteDuration::Musical(_) => CoordinateKind::Musical,
            ConcreteDuration::WallClock(_) => CoordinateKind::WallClock,
        }
    }
}

impl EventDuration {
    /// The concrete clock of a determinate duration, or `None` for an
    /// indeterminate one.
    pub fn concrete_kind(&self) -> Option<CoordinateKind> {
        match self {
            EventDuration::Musical(_) => Some(CoordinateKind::Musical),
            EventDuration::WallClock(_) => Some(CoordinateKind::WallClock),
            EventDuration::Indeterminate(_) => None,
        }
    }
}

/// A bounded interval expressing an indeterminate duration (Chapter 5). Bounds
/// are [`ConcreteDuration`] so indeterminacy cannot recurse.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct DurationBounds {
    pub lower: Option<ConcreteDuration>,
    pub upper: Option<ConcreteDuration>,
}

/// An interval bound for an aleatoric event's start or end (Chapter 3
/// §"Aleatoric Time").
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum TimeBounds {
    MusicalRange {
        min: MusicalPosition,
        max: MusicalPosition,
    },
    WallClockRange {
        min: WallClockTime,
        max: WallClockTime,
    },
    Unbounded,
}

/// Per-event interval bounds for an aleatoric region (Chapter 3 §"Aleatoric
/// Time"): an event may begin/end anywhere within the given windows.
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct EventBounds {
    pub start: Option<TimeBounds>,
    pub end: Option<TimeBounds>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(n: i64, d: i64) -> RationalTime {
        RationalTime::new(n, d).unwrap()
    }

    #[test]
    fn rationals_normalize_and_compare_by_value() {
        assert_eq!(r(2, 4), r(1, 2));
        assert_eq!(r(-3, -6), r(1, 2));
        assert!(r(1, 3) < r(1, 2));
        assert!(r(-1, 2) < RationalTime::zero());
        // 1/4 + 1/12 = 1/3 (the tuplet-eighth example, Chapter 3).
        assert_eq!(r(1, 4).add(&r(1, 12)), r(1, 3));
        // A quintuplet-sixteenth in a duplet-half: 1/2 * 1/2 * 1/5 = 1/20.
        assert_eq!(r(1, 2).mul(&r(1, 2)).mul(&r(1, 5)), r(1, 20));
    }

    #[test]
    fn arithmetic_promotes_then_stays_exact() {
        // Denominators 999_999_937 (prime) and 999_999_893 (prime) multiply to
        // ~1e18, far past u32; the result must promote and stay exact.
        let a = r(1, 999_999_937);
        let b = r(1, 999_999_893);
        let sum = a.add(&b);
        assert!(matches!(sum, RationalTime::Large(_)), "must promote");
        // Cross-check against an independent BigRational computation.
        let expect = BigRational::new(BigInt::from(1), BigInt::from(999_999_937i64))
            + BigRational::new(BigInt::from(1), BigInt::from(999_999_893i64));
        assert_eq!(sum, RationalTime::from_big(expect));
        // Subtracting back demotes to the inline value, unobservably.
        let back = sum.sub(&b);
        assert_eq!(back, a);
        assert!(matches!(back, RationalTime::Small(_)), "must demote");
    }

    #[test]
    fn equal_values_hash_equally_across_construction_paths() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let h = |x: &RationalTime| {
            let mut s = DefaultHasher::new();
            x.hash(&mut s);
            s.finish()
        };
        assert_eq!(h(&r(2, 4)), h(&r(1, 2)));
        assert_eq!(h(&r(6, 3)), h(&RationalTime::from_int(2)));
    }

    #[test]
    fn position_duration_algebra_is_typed() {
        let p = MusicalPosition(r(1, 2));
        let d = MusicalDuration(r(1, 4));
        let p2 = p.clone() + d.clone(); // position + duration -> position
        assert_eq!(p2, MusicalPosition(r(3, 4)));
        let span = p2 - p; // position - position -> duration
        assert_eq!(span, MusicalDuration(r(1, 4)));
        let dd = d.clone() + d; // duration + duration -> duration
        assert_eq!(dd, MusicalDuration(r(1, 2)));
    }

    #[test]
    fn rational_round_trips_canonically_including_large() {
        for v in [
            RationalTime::zero(),
            r(1, 1),
            r(-7, 12),
            r(3, 1024),
            r(1, 999_999_937).add(&r(1, 999_999_893)),
        ] {
            let bytes = v.to_canonical_bytes();
            let back = RationalTime::decode_canonical(&bytes).unwrap();
            assert_eq!(back, v);
            assert_eq!(back.to_canonical_bytes(), bytes, "re-encode byte-stable");
        }
    }

    #[test]
    fn equal_rationals_encode_identically() {
        assert_eq!(r(2, 4).to_canonical_bytes(), r(1, 2).to_canonical_bytes());
        assert_eq!(
            r(6, 3).to_canonical_bytes(),
            RationalTime::from_int(2).to_canonical_bytes()
        );
    }

    #[test]
    fn wallclock_round_trips() {
        for v in [i64::MIN, -1, 0, 1, 1_000_000_000, i64::MAX] {
            let t = WallClockTime(v);
            assert_eq!(
                WallClockTime::decode_canonical(&t.to_canonical_bytes()).unwrap(),
                t
            );
        }
    }
}
