//! Drives the canonical encode/decode round-trip fuzz harness from the command
//! line, so the 1M-iteration hand-off gate (and larger soak runs) can run
//! outside the unit-test timeout.
//!
//! Usage:
//!   cargo run --release --example fuzz_roundtrip [ITERS] [SEED]
//!
//! Defaults: 1_000_000 iterations, seed 0. Exits non-zero (via panic) on the
//! first round-trip violation.

use epiphany_determinism::fuzz::run_round_trip_fuzz;

fn main() {
    let mut args = std::env::args().skip(1);
    let iters: u64 = args
        .next()
        .map(|s| s.parse().expect("ITERS must be an integer"))
        .unwrap_or(1_000_000);
    let seed: u64 = args
        .next()
        .map(|s| s.parse().expect("SEED must be an integer"))
        .unwrap_or(0);

    eprintln!("round-trip fuzz: {iters} iterations, seed {seed}");
    run_round_trip_fuzz(iters, seed);
    eprintln!("ok: {iters} iterations, no round-trip violations");
}
