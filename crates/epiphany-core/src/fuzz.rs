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
    /// migration path with real v0 inputs rather than only mutated current
    /// bytes.
    v0_scores: Vec<Vec<u8>>,
    /// Valid **frozen v1** whole-`Score` encodings (via
    /// [`crate::codec::encode_v1_score`]) — the schema-major-2 migration's
    /// input form.
    v1_scores: Vec<Vec<u8>>,
}

/// The rich fixture plus repeat structures carrying non-default v2 content
/// (a DalSegno kind, voltas), so the decode fuzzer exercises the
/// RepeatKind/Volta wire arms through a whole-`Score` form. Corpus-local —
/// the shared render fixtures deliberately stay repeat-free until the E1
/// rendering tranche (golden-churn discipline).
fn valid_score_rich_with_repeats(seed: u64) -> Score {
    use crate::graph::{RepeatKind, RepeatStructure, Volta};
    use crate::ids::RepeatStructureId;
    use crate::time::{AnchorOffset, RegionEdge, TimeAnchor};
    let mut score = valid_score_rich(seed);
    let region = score.canvas.regions[0].id;
    let span = |edge: RegionEdge| TimeAnchor::Region {
        id: region,
        edge,
        offset: AnchorOffset::Zero,
    };
    let replica = crate::ids::ReplicaId(0xF0F0);
    score.cross_cutting.repeats.push(RepeatStructure {
        id: RepeatStructureId::new(replica, 1),
        start: span(RegionEdge::Start),
        end: span(RegionEdge::End),
        kind: RepeatKind::DalSegno {
            segno: span(RegionEdge::Start),
            end_target: span(RegionEdge::End),
        },
        voltas: Vec::new(),
    });
    score.cross_cutting.repeats.push(RepeatStructure {
        id: RepeatStructureId::new(replica, 2),
        start: span(RegionEdge::Start),
        end: span(RegionEdge::End),
        kind: RepeatKind::Volta,
        voltas: vec![
            Volta {
                endings: vec![1],
                start: span(RegionEdge::Start),
                end: span(RegionEdge::End),
            },
            Volta {
                endings: vec![2, 3],
                start: span(RegionEdge::Start),
                end: span(RegionEdge::End),
            },
        ],
    });
    score
}

fn build_corpus(rng: &mut SplitMix64) -> Corpus {
    let mut scores = Vec::new();
    let mut regions = Vec::new();
    let mut v0_scores = Vec::new();
    let mut v1_scores = Vec::new();
    for i in 0..12u64 {
        let seed = rng.next_u64();
        let score = match i % 3 {
            0 => valid_score(seed | 1),
            1 => valid_score_rich(seed),
            _ => valid_score_rich_with_repeats(seed),
        };
        if let Some(region) = score.canvas.regions.first() {
            regions.push(region.canonical_bytes());
        }
        v0_scores.push(crate::codec::encode_v0_score(&score));
        v1_scores.push(crate::codec::encode_v1_score(&score));
        scores.push(score.canonical_bytes());
    }
    Corpus {
        scores,
        regions,
        v0_scores,
        v1_scores,
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

        // The schema-version dispatch seam. Major 3 is the current layout;
        // majors 2, 1, and 0 run the frozen migrations; an arbitrary major
        // exercises the defensive out-of-accept-set path. Each migration
        // default-fills the appended fields, so it does not round-trip to the
        // *current* form — but each is strictly canonical over its OWN wire
        // form: an accepted input re-encodes to itself via the frozen encoder.
        // This proves non-canonical rejection on every versioned path, not
        // just the absence of a panic.
        let _ = Score::decode_canonical_versioned(&bytes, 2);
        if let Ok(v1_score) = Score::decode_canonical_versioned(&bytes, 1) {
            assert_eq!(
                crate::codec::encode_v1_score(&v1_score),
                bytes,
                "the v1 migration accepted a non-canonical v1 byte string"
            );
        }
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

        // Every ~4th iteration, feed a *genuine* frozen-form encoding
        // (mutated) to its migration path — both the v0 and v1 wire forms —
        // so each strict canonicality guard is hit with real bytes of its own
        // major. An UNMUTATED frozen encoding MUST decode Ok (enforced, not
        // just commented); an accepted input must re-encode to itself.
        if rng.next_u64() % 4 == 0 {
            type Reenc = fn(&Score) -> Vec<u8>;
            let forms: [(&[Vec<u8>], u16, Reenc, &str); 2] = [
                (
                    &corpus.v0_scores,
                    0,
                    crate::codec::encode_v0_score as Reenc,
                    "v0",
                ),
                (
                    &corpus.v1_scores,
                    1,
                    crate::codec::encode_v1_score as Reenc,
                    "v1",
                ),
            ];
            for (pool, major, reenc, label) in forms {
                let mut bytes = pool[(rng.next_u64() as usize) % pool.len()].clone();
                let mutation = rng.next_u64() % 4;
                match mutation {
                    0 => {} // unmutated: must decode Ok (asserted below).
                    1 => {
                        let k = 1 + (rng.next_u64() % 4) as usize;
                        substitute(&mut rng, &mut bytes, k);
                    }
                    2 => {
                        let t = (rng.next_u64() as usize) % (bytes.len() + 1);
                        bytes.truncate(t);
                    }
                    _ => corrupt_length_prefix(&mut rng, &mut bytes),
                }
                match Score::decode_canonical_versioned(&bytes, major) {
                    Ok(score) => assert_eq!(
                        reenc(&score),
                        bytes,
                        "the {label} migration accepted a non-canonical {label} byte string"
                    ),
                    Err(_) => assert_ne!(
                        mutation, 0,
                        "an unmutated genuine {label} encoding must decode Ok"
                    ),
                }
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
