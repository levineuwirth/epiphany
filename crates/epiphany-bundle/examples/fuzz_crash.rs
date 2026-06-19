//! Drives the crash-recovery fuzzer from the command line, so the
//! 10,000-iteration acceptance gate (and larger soak runs) can run outside the
//! unit-test timeout — the analogue of `epiphany-determinism`'s
//! `examples/fuzz_roundtrip`.
//!
//! Usage:
//!   cargo run --release --example fuzz_crash [ITERS] [SEED]
//!
//! Defaults: 10_000 iterations, seed 0. Exits non-zero (via panic) on the first
//! recovery violation; the failing iteration reproduces exactly from its seed.

use epiphany_bundle::fuzz::run_crash_recovery_fuzz;

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

    eprintln!("crash-recovery fuzz: {iters} iterations, seed {seed}");
    run_crash_recovery_fuzz(iters, seed);
    eprintln!("ok: {iters} iterations, every crash recovered to a valid bundle");
}
