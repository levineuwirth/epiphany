//! Drives the reduction-determinism and equivocation fuzz harnesses from the
//! command line, so the hand-off gates (and larger soak runs) can run outside
//! the unit-test timeout.
//!
//! Usage:
//!   cargo run --release --example fuzz_reduction [ITERS] [SEED]
//!
//! Defaults: 10_000 iterations, seed 0. Runs the reduction-determinism harness
//! then the equivocation harness. Exits non-zero (via panic) on the first
//! violation — a permutation that reduced to different bytes, or a
//! duplicate-id-with-different-bytes that failed to equivocate.

use epiphany_ops::fuzz::{run_equivocation_fuzz, run_reduction_determinism_fuzz};

fn main() {
    let mut args = std::env::args().skip(1);
    let iters: u64 = args
        .next()
        .map(|s| s.parse().expect("ITERS must be an integer"))
        .unwrap_or(10_000);
    let seed: u64 = args
        .next()
        .map(|s| s.parse().expect("SEED must be an integer"))
        .unwrap_or(0);

    eprintln!("reduction-determinism fuzz: {iters} iterations, seed {seed}");
    run_reduction_determinism_fuzz(iters, seed);
    eprintln!("ok: reduction is permutation-invariant across {iters} sets");

    eprintln!("equivocation fuzz: {iters} iterations, seed {seed}");
    run_equivocation_fuzz(iters, seed);
    eprintln!("ok: equivocation is order-independent across {iters} sets");
}
