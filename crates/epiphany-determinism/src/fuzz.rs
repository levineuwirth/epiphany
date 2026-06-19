//! Deterministic round-trip fuzz harness.
//!
//! The Agent A hand-off gate (QUICKSTART): *"the fuzz harness for round-trip
//! canonical encode/decode runs 1M iterations without panic."* This module is
//! that harness. It is `pub` so the workspace's `xtask`/testkit can drive it
//! at scale outside the unit-test timeout, and the `examples/fuzz_roundtrip`
//! binary runs it from the command line.
//!
//! The harness is itself deterministic: it draws inputs from a seeded
//! SplitMix64 generator (no `rand` dependency, no platform entropy), so a
//! failing iteration reproduces exactly from its seed. The properties checked
//! each iteration:
//!
//! 1. **Round-trip.** For a canonical value `x`, `decode(encode(x)) == x`.
//! 2. **Re-encode stability.** `encode(decode(encode(x))) == encode(x)` —
//!    canonical bytes are stable under a decode/encode cycle.
//! 3. **Rejection.** Non-finite float payloads decode to an error, never to a
//!    NaN/inf masquerading as canonical state.

use crate::coord::QuantizedCoord;
use crate::domain::DomainTag;
use crate::float::CanonicalF64;
use crate::hash::{ChunkId, ContentHash};
use crate::serialize::{CanonicalDecode, CanonicalEncode, DecodeError};

/// A tiny deterministic generator (SplitMix64). Reproducible across platforms;
/// used only to drive the harness, never to produce canonical state.
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    /// Seeds the generator.
    pub fn new(seed: u64) -> Self {
        SplitMix64 { state: seed }
    }

    /// Next 64-bit draw.
    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// 32 pseudo-random bytes (one content-hash worth).
    #[inline]
    fn next_32(&mut self) -> [u8; 32] {
        let mut out = [0u8; 32];
        for chunk in out.chunks_mut(8) {
            chunk.copy_from_slice(&self.next_u64().to_le_bytes());
        }
        out
    }
}

/// Round-trips one canonical value through encode/decode, asserting the value
/// survives and its bytes are stable. Returns the encoded length so callers
/// can sanity-check fixed widths.
fn check_round_trip<T>(value: &T, scratch: &mut Vec<u8>) -> usize
where
    T: CanonicalEncode + CanonicalDecode + PartialEq + core::fmt::Debug,
{
    scratch.clear();
    value.encode_canonical(scratch);
    let decoded =
        T::decode_canonical(scratch).unwrap_or_else(|e| panic!("decode of {value:?} failed: {e}"));
    assert_eq!(&decoded, value, "round-trip changed the value");
    let re_encoded = decoded.to_canonical_bytes();
    assert_eq!(
        re_encoded.as_slice(),
        scratch.as_slice(),
        "re-encode not byte-identical"
    );
    scratch.len()
}

/// Runs `iters` round-trip iterations from `seed`. Panics on the first
/// violation (which is the hand-off gate's failure condition); returns
/// normally — having touched every determinism-owned encode/decode primitive —
/// when all iterations hold.
pub fn run_round_trip_fuzz(iters: u64, seed: u64) {
    let mut rng = SplitMix64::new(seed);
    let mut scratch = Vec::with_capacity(64);

    for _ in 0..iters {
        match rng.next_u64() % 6 {
            // QuantizedCoord: every i64 is valid; fixed 8-byte width.
            0 => {
                let q = QuantizedCoord::from_units(rng.next_u64() as i64);
                assert_eq!(check_round_trip(&q, &mut scratch), 8);
            }
            // QuantizedCoord via the quantization path, in a realistic range
            // where the staff-space round-trip is also exact.
            1 => {
                let units = (rng.next_u64() % (1 << 40)) as i64 - (1 << 39);
                let q = QuantizedCoord::from_units(units);
                // Quantizing the de-quantized value is the identity here.
                assert_eq!(
                    QuantizedCoord::from_staff_spaces(q.to_staff_spaces()),
                    Some(q)
                );
                check_round_trip(&q, &mut scratch);
            }
            // CanonicalF64: arbitrary bits split into the finite and
            // non-finite cases.
            2 => {
                let bits = rng.next_u64();
                let raw = f64::from_bits(bits);
                match CanonicalF64::new(raw) {
                    Some(c) => {
                        assert_eq!(check_round_trip(&c, &mut scratch), 8);
                    }
                    None => {
                        // Non-finite payload must be rejected on decode.
                        assert!(!raw.is_finite());
                        let err = CanonicalF64::decode_canonical(&raw.to_le_bytes());
                        assert_eq!(err, Err(DecodeError::NonFiniteFloat));
                    }
                }
            }
            // ContentHash: any 32 bytes are valid.
            3 => {
                let h = ContentHash(rng.next_32());
                assert_eq!(check_round_trip(&h, &mut scratch), 32);
            }
            // ChunkId: newtype over ContentHash.
            4 => {
                let id = ChunkId(ContentHash(rng.next_32()));
                assert_eq!(check_round_trip(&id, &mut scratch), 32);
            }
            // DomainTag: valid tags are a built-in or a MUSCS extension tag
            // (printable ASCII). Generate one, round-trip it; also confirm a
            // foreign 8 bytes is rejected on decode.
            _ => {
                let builtins = DomainTag::BUILTINS;
                let pick = (rng.next_u64() as usize) % (builtins.len() + 1);
                let tag = if pick < builtins.len() {
                    builtins[pick]
                } else {
                    // "MUSCS" + 3 uppercase ASCII letters: a valid system tag.
                    let mut raw = *b"MUSCSAAA";
                    for byte in raw[5..8].iter_mut() {
                        *byte = b'A' + (rng.next_u64() % 26) as u8;
                    }
                    DomainTag::from_bytes(raw).unwrap_or_else(|| {
                        // The reserved superblock magic is also `MUSCS...`;
                        // use a known-valid extension tag if the random draw
                        // collides with a reserved `MUSC*` constant.
                        DomainTag::from_bytes(*b"MUSCSEXT").expect("fallback extension tag")
                    })
                };
                assert_eq!(check_round_trip(&tag, &mut scratch), 8);

                let mut bad = rng.next_u64().to_le_bytes();
                bad[0] = b'X'; // guarantee no MUSC prefix
                assert_eq!(
                    DomainTag::decode_canonical(&bad),
                    Err(DecodeError::MalformedDomainTag)
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Stable seed for the gate run; any fixed value works since the harness
    /// is deterministic.
    const GATE_SEED: u64 = 0xE91F_A012_3456_789A;

    /// The hand-off gate: 1,000,000 round-trip iterations without panic. Cheap
    /// fixed-width ops with a reused scratch buffer, so this is fast.
    #[test]
    fn fuzz_round_trip_one_million_iterations() {
        run_round_trip_fuzz(1_000_000, GATE_SEED);
    }

    #[test]
    fn generator_is_deterministic() {
        let mut a = SplitMix64::new(42);
        let mut b = SplitMix64::new(42);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }
}
