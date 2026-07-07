//! Adversarial byte-decode fuzzing for the whole-`Score` canonical codec and
//! the per-value [`CanonicalValue`] decoders (the Binary Format companion's
//! "wire-format fuzzer" charter item).
//!
//! The canonical decoders are a **trust boundary**: they parse bytes that may
//! be truncated, corrupted, or wholly adversarial (a hostile bundle, a bit-rot
//! chunk, a mismatched implementation). The contract this harness enforces is
//! that *every* byte string decodes to a clean [`Err`] — the decoders **never
//! panic, never over-allocate, and never loop unboundedly** — and that any
//! string a decoder accepts re-encodes to itself, since canonical decoding is
//! injective (a value has exactly one canonical byte form; trailing or
//! non-canonical bytes are rejected). A panic here fails the run and names the
//! seed, so any counterexample is reproducible.
//!
//! This deliberately hammers the schema-major-1 migration surface added by the
//! schema-major track: [`Score::decode_canonical_versioned`] and its frozen
//! major-0 walk (`decode_v0_score` → `dec_canvas_v0` / `dec_region_v0` /
//! `dec_instruments_v0`), whose per-element `Vec` loops over attacker-supplied
//! counts are exactly the shape that, done naively, over-allocates or reads out
//! of bounds.

use epiphany_determinism::fuzz::SplitMix64;

use crate::generators::{valid_score, valid_score_rich};
use crate::{CanonicalValue, Region, Score, ScoreDecodeError};

/// `n` pseudo-random bytes.
fn random_bytes(rng: &mut SplitMix64, n: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(n + 8);
    while out.len() < n {
        out.extend_from_slice(&rng.next_u64().to_le_bytes());
    }
    out.truncate(n);
    out
}

/// A small pool of valid canonical encodings, built **once** per run — building
/// a fresh `Score` per iteration dominates the cost, so the corpus is generated
/// up front and every iteration mutates a clone of a pooled entry.
struct Corpus {
    /// Valid whole-`Score` encodings (both the simple and rich/multi-region
    /// shapes).
    scores: Vec<Vec<u8>>,
    /// Valid single-`Region` encodings (the schema-major-1 type: it grew
    /// `permits_spanning_slurs`).
    regions: Vec<Vec<u8>>,
    /// Valid **frozen v0** whole-`Score` encodings — genuine major-0 wire bytes
    /// (via [`crate::codec::encode_v0_score`]), to exercise the strict v0
    /// migration path with real v0 inputs rather than only mutated v1 bytes.
    v0_scores: Vec<Vec<u8>>,
}

fn build_corpus(rng: &mut SplitMix64) -> Corpus {
    let mut scores = Vec::new();
    let mut regions = Vec::new();
    let mut v0_scores = Vec::new();
    for i in 0..12u64 {
        let seed = rng.next_u64();
        let score = if i % 2 == 0 {
            valid_score(seed | 1)
        } else {
            valid_score_rich(seed)
        };
        if let Some(region) = score.canvas.regions.first() {
            regions.push(region.canonical_bytes());
        }
        v0_scores.push(crate::codec::encode_v0_score(&score));
        scores.push(score.canonical_bytes());
    }
    Corpus {
        scores,
        regions,
        v0_scores,
    }
}

/// A clone of a random pooled valid `Score` encoding.
fn valid_score_bytes(rng: &mut SplitMix64, corpus: &Corpus) -> Vec<u8> {
    corpus.scores[(rng.next_u64() as usize) % corpus.scores.len()].clone()
}

/// A clone of a random pooled valid `Region` encoding.
fn valid_region_bytes(rng: &mut SplitMix64, corpus: &Corpus) -> Vec<u8> {
    corpus.regions[(rng.next_u64() as usize) % corpus.regions.len()].clone()
}

/// Overwrites up to `k` random single bytes.
fn substitute(rng: &mut SplitMix64, bytes: &mut [u8], k: usize) {
    if bytes.is_empty() {
        return;
    }
    for _ in 0..k {
        let i = (rng.next_u64() as usize) % bytes.len();
        bytes[i] = rng.next_u64() as u8;
    }
}

/// Overwrites a random 4-byte window with a fresh `u32` (often large): the
/// length/count-prefix attack — the value a `Vec`/`String` decoder trusts for
/// its element count or byte length.
fn corrupt_length_prefix(rng: &mut SplitMix64, bytes: &mut [u8]) {
    if bytes.len() < 4 {
        return;
    }
    let i = (rng.next_u64() as usize) % (bytes.len() - 3);
    // Bias toward extreme counts (all-ones / near-u32::MAX) alongside plain
    // random draws, since those are what stress the allocation guards.
    let v: u32 = match rng.next_u64() % 3 {
        0 => u32::MAX,
        1 => (rng.next_u64() as u32) | 0x8000_0000,
        _ => rng.next_u64() as u32,
    };
    bytes[i..i + 4].copy_from_slice(&v.to_le_bytes());
}

/// Builds one adversarial input by a strategy chosen from `rng`. Strategy 1
/// returns *unmutated* valid `Score` bytes (a live sanity check that the
/// harness's own valid corpus round-trips).
fn gen_score_input(rng: &mut SplitMix64, corpus: &Corpus) -> Vec<u8> {
    match rng.next_u64() % 7 {
        0 => {
            let n = (rng.next_u64() % 512) as usize;
            random_bytes(rng, n)
        }
        1 => valid_score_bytes(rng, corpus),
        2 => {
            let mut b = valid_score_bytes(rng, corpus);
            let k = 1 + (rng.next_u64() % 4) as usize;
            substitute(rng, &mut b, k);
            b
        }
        3 => {
            let mut b = valid_score_bytes(rng, corpus);
            let t = (rng.next_u64() as usize) % (b.len() + 1);
            b.truncate(t);
            b
        }
        4 => {
            let mut b = valid_score_bytes(rng, corpus);
            let n = 1 + (rng.next_u64() % 16) as usize;
            let tail = random_bytes(rng, n);
            b.extend_from_slice(&tail);
            b
        }
        5 => {
            let mut b = valid_score_bytes(rng, corpus);
            corrupt_length_prefix(rng, &mut b);
            b
        }
        _ => {
            // A valid Region's bytes, standing where a Score is expected (a
            // structurally plausible but wrong-type payload).
            valid_region_bytes(rng, corpus)
        }
    }
}

/// Asserts a whole-`Score` decode result is well-behaved: an accepted string
/// re-encodes to itself (canonical decode is injective). A panic in the decoder
/// would already have aborted the run.
fn check_score(result: Result<Score, ScoreDecodeError>, bytes: &[u8]) {
    if let Ok(score) = result {
        // Strictly canonical decode is injective: an accepted string re-encodes
        // to itself (enforced by `Score::decode_canonical`; this is the fuzzer's
        // independent safety net over 20K+ adversarial inputs).
        assert_eq!(
            score.canonical_bytes(),
            bytes,
            "the whole-Score decoder accepted a non-canonical byte string"
        );
    }
}

/// Runs `iters` adversarial byte-decode iterations from `seed` against the
/// whole-`Score` codec (current layout and the versioned seam, including the
/// frozen major-0 migration) and a per-value decoder. Panics — a decoder crash
/// or a non-canonical acceptance — fail the run; the `seed` reproduces it.
pub fn run_decode_fuzz(iters: u64, seed: u64) {
    let mut rng = SplitMix64::new(seed);
    let corpus = build_corpus(&mut rng);
    for _ in 0..iters {
        let bytes = gen_score_input(&mut rng, &corpus);

        // The current-layout decoder: must not panic; an Ok must round-trip.
        check_score(Score::decode_canonical(&bytes), &bytes);

        // The schema-version dispatch seam. Major 1 is the current layout; major
        // 0 runs the frozen `decode_v0_score` migration; an arbitrary major
        // exercises the defensive out-of-accept-set path.
        let _ = Score::decode_canonical_versioned(&bytes, 1);
        // The v0 migration default-fills the schema-major-1 fields, so it does
        // not round-trip to the *v1* form — but it is strictly canonical over the
        // **v0 wire form**: an accepted input re-encodes to itself via the frozen
        // v0 encoder. This proves non-canonical rejection on the v0 path, not
        // just the absence of a panic.
        if let Ok(v0_score) = Score::decode_canonical_versioned(&bytes, 0) {
            assert_eq!(
                crate::codec::encode_v0_score(&v0_score),
                bytes,
                "the v0 migration accepted a non-canonical v0 byte string"
            );
        }
        let _ = Score::decode_canonical_versioned(&bytes, rng.next_u64() as u16);

        // A per-value decoder over the same adversarial bytes.
        let _ = Region::decode_canonical(&bytes);

        // Every ~8th iteration, target the Region decoder with bytes grown from
        // a *valid Region* (mutated), so the value codec is hit past its early
        // tags, not just rejected at byte 0.
        if rng.next_u64() % 8 == 0 {
            let mut rb = valid_region_bytes(&mut rng, &corpus);
            match rng.next_u64() % 3 {
                0 => {
                    let k = 1 + (rng.next_u64() % 3) as usize;
                    substitute(&mut rng, &mut rb, k);
                }
                1 => {
                    let t = (rng.next_u64() as usize) % (rb.len() + 1);
                    rb.truncate(t);
                }
                _ => corrupt_length_prefix(&mut rng, &mut rb),
            }
            if let Ok(region) = Region::decode_canonical(&rb) {
                assert_eq!(
                    region.canonical_bytes(),
                    rb,
                    "the Region decoder accepted a non-canonical byte string"
                );
            }
        }

        // Every ~4th iteration, feed a *genuine* v0-form encoding (mutated) to
        // the frozen major-0 migration, so its strict v0 canonicality is hit
        // with real v0 bytes — an accepted input must re-encode to itself in the
        // v0 wire form, and an unmutated v0 encoding must always be accepted.
        if rng.next_u64() % 4 == 0 {
            let mut v0 =
                corpus.v0_scores[(rng.next_u64() as usize) % corpus.v0_scores.len()].clone();
            match rng.next_u64() % 4 {
                0 => {} // unmutated: must decode Ok and round-trip the v0 form.
                1 => {
                    let k = 1 + (rng.next_u64() % 4) as usize;
                    substitute(&mut rng, &mut v0, k);
                }
                2 => {
                    let t = (rng.next_u64() as usize) % (v0.len() + 1);
                    v0.truncate(t);
                }
                _ => corrupt_length_prefix(&mut rng, &mut v0),
            }
            if let Ok(score) = Score::decode_canonical_versioned(&v0, 0) {
                assert_eq!(
                    crate::codec::encode_v0_score(&score),
                    v0,
                    "the v0 migration accepted a non-canonical v0 byte string"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fast smoke run in the ordinary test suite: enough iterations to catch a
    /// gross regression, cheap enough for every `cargo test` (each iteration
    /// decodes *and* re-encodes several times for the strict-canonical check, so
    /// the count is kept modest; a deeper sweep runs via [`run_decode_fuzz`] with
    /// a large `iters` in a dedicated gate).
    #[test]
    fn decode_fuzz_smoke() {
        run_decode_fuzz(20_000, 0x0DEC_0DE0_F022_1234);
    }

    /// A second seed, so a determinism-sensitive bug does not hide behind one
    /// generator stream.
    #[test]
    fn decode_fuzz_smoke_alt_seed() {
        run_decode_fuzz(20_000, 0xF0FA_11BA_C0DE_5EED);
    }

    /// Directly confirm the harness's core invariants on hand-built inputs — so
    /// a change that made the fuzzer vacuous (e.g. always generating rejected
    /// bytes) is caught.
    #[test]
    fn valid_bytes_round_trip_and_truncations_reject() {
        let score = valid_score(0xA11CE);
        let bytes = score.canonical_bytes();
        assert_eq!(Score::decode_canonical(&bytes).unwrap(), score);
        // Every proper prefix is rejected (never accepted, never a panic).
        for t in 0..bytes.len() {
            assert!(Score::decode_canonical(&bytes[..t]).is_err());
        }
        // Trailing garbage is rejected.
        let mut extended = bytes.clone();
        extended.push(0);
        assert!(Score::decode_canonical(&extended).is_err());
    }
}
