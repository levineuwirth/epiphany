//! The testkit's single source of pseudo-randomness.
//!
//! Appendix D §"Randomness" forbids platform entropy from reaching canonical
//! state; the testkit holds itself to the stronger rule that **no platform
//! entropy enters the harness at all**, so every generated case reproduces
//! exactly from its seed. All draws route through [`Rng`], a thin ergonomic
//! wrapper over Agent A's vendored SplitMix64
//! ([`epiphany_determinism::fuzz::SplitMix64`]) — the same generator the
//! determinism and bundle fuzzers use, so seeds are comparable across crates.

use epiphany_determinism::fuzz::SplitMix64;

/// A seeded, fully deterministic generator with the small set of draws the
/// testkit's generators and harnesses need. Reproducible across platforms.
pub struct Rng {
    inner: SplitMix64,
}

impl Rng {
    /// Seeds the generator. Identical seeds produce identical streams.
    pub fn new(seed: u64) -> Self {
        Rng {
            inner: SplitMix64::new(seed),
        }
    }

    /// A raw 64-bit draw.
    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        self.inner.next_u64()
    }

    /// A draw in `0..n`. Panics if `n == 0`.
    #[inline]
    pub fn below(&mut self, n: u64) -> u64 {
        assert!(n > 0, "Rng::below requires a positive bound");
        // Unbiased: reject the top partial bucket so every residue is equally
        // likely (plain `% n` over-weights the low residues). `n.wrapping_neg()
        // % n == 2^64 mod n` is the size of that partial bucket.
        if n.is_power_of_two() {
            return self.next_u64() & (n - 1);
        }
        let reject_below = n.wrapping_neg() % n;
        loop {
            let x = self.next_u64();
            if x >= reject_below {
                return x % n;
            }
        }
    }

    /// An inclusive draw in `lo..=hi`. Panics if `lo > hi`. Handles the full
    /// `0..=u64::MAX` span without the `hi - lo + 1` overflow.
    #[inline]
    pub fn range(&mut self, lo: u64, hi: u64) -> u64 {
        assert!(lo <= hi, "Rng::range requires lo <= hi");
        let span = hi - lo;
        if span == u64::MAX {
            return self.next_u64();
        }
        lo + self.below(span + 1)
    }

    /// An inclusive `usize` draw in `lo..=hi`.
    #[inline]
    pub fn range_usize(&mut self, lo: usize, hi: usize) -> usize {
        self.range(lo as u64, hi as u64) as usize
    }

    /// A fair coin.
    #[inline]
    pub fn boolean(&mut self) -> bool {
        self.next_u64() & 1 == 0
    }

    /// A reference to a uniformly chosen element. Panics on an empty slice.
    #[inline]
    pub fn choose<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        assert!(!items.is_empty(), "Rng::choose on an empty slice");
        &items[self.below(items.len() as u64) as usize]
    }

    /// `len` pseudo-random bytes.
    pub fn bytes(&mut self, len: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(len);
        while out.len() < len {
            let word = self.next_u64().to_le_bytes();
            let take = (len - out.len()).min(8);
            out.extend_from_slice(&word[..take]);
        }
        out
    }

    /// A vector of `lo..=hi` pseudo-random bytes (length drawn first). Avoids
    /// the nested-`&mut self` borrow of `bytes(range_usize(..))`.
    pub fn byte_vec(&mut self, lo: usize, hi: usize) -> Vec<u8> {
        let len = self.range_usize(lo, hi);
        self.bytes(len)
    }

    /// A fixed 32-byte draw (one content-hash worth).
    pub fn array32(&mut self) -> [u8; 32] {
        let mut out = [0u8; 32];
        for chunk in out.chunks_mut(8) {
            chunk.copy_from_slice(&self.next_u64().to_le_bytes());
        }
        out
    }

    /// A fixed 16-byte draw (one document-id / file-uuid worth).
    pub fn array16(&mut self) -> [u8; 16] {
        let mut out = [0u8; 16];
        out[..8].copy_from_slice(&self.next_u64().to_le_bytes());
        out[8..].copy_from_slice(&self.next_u64().to_le_bytes());
        out
    }

    /// Fisher–Yates in-place shuffle. Deterministic for a given seed; this is
    /// how the convergence harness produces its *N random delivery orders*.
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        let n = items.len();
        if n < 2 {
            return;
        }
        for i in (1..n).rev() {
            // Unbiased draw in 0..=i (routes through `below`'s rejection sampling
            // rather than a raw modulo).
            let j = self.below(i as u64 + 1) as usize;
            items.swap(i, j);
        }
    }

    /// A fresh permutation of `0..n`.
    pub fn permutation(&mut self, n: usize) -> Vec<usize> {
        let mut v: Vec<usize> = (0..n).collect();
        self.shuffle(&mut v);
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_deterministic() {
        let mut a = Rng::new(1234);
        let mut b = Rng::new(1234);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn shuffle_is_a_permutation() {
        let mut rng = Rng::new(7);
        let mut v: Vec<usize> = (0..50).collect();
        rng.shuffle(&mut v);
        let mut sorted = v.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..50).collect::<Vec<_>>());
        // And it actually moved something (astronomically unlikely to be identity).
        assert_ne!(v, (0..50).collect::<Vec<_>>());
    }

    #[test]
    fn bounded_draws_stay_in_range() {
        let mut rng = Rng::new(99);
        for _ in 0..10_000 {
            let lo = rng.below(10);
            let hi = lo + rng.below(10);
            let x = rng.range(lo, hi);
            assert!(lo <= x && x <= hi);
        }
    }

    #[test]
    fn full_range_does_not_overflow() {
        // `range(0, u64::MAX)` must not compute `hi - lo + 1` (which overflows,
        // and panics under the workspace's release overflow-checks).
        let mut rng = Rng::new(123);
        for _ in 0..1000 {
            let _ = rng.range(0, u64::MAX);
        }
        // And a near-full span.
        let _ = rng.range(5, u64::MAX);
    }

    #[test]
    fn below_is_approximately_uniform() {
        // A coarse chi-square-free check: each of n buckets gets a fair share.
        let mut rng = Rng::new(0xBEEF);
        let n = 7u64; // non-power-of-two exercises the rejection path
        let trials = 70_000u64;
        let mut counts = [0u64; 7];
        for _ in 0..trials {
            counts[rng.below(n) as usize] += 1;
        }
        let expected = trials / n;
        for c in counts {
            // Within 10% of the expected share — loose, just catches gross bias.
            assert!(
                c > expected * 9 / 10 && c < expected * 11 / 10,
                "bucket count {c} far from expected {expected}"
            );
        }
    }
}
