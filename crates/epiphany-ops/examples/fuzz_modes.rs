//! The two-mode fuzz at a chosen budget (`epiphany_ops::fuzz::modes`):
//!
//!     cargo run --release -p epiphany-ops --example fuzz_modes -- \
//!         [ITERATIONS] [SEED] [AUTHORED] [--effects] [--minimize DIR]
//!
//! Prints each class of failure once with the first seed that showed it, and
//! how many of each kind were authored and applied (with `--effects`, what
//! became of the rest). With `--minimize DIR`,
//! writes each class's history, minimized, to `DIR/<n>.txt`. Exits nonzero
//! when anything was found.

use epiphany_ops::fuzz::modes;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let minimize = args
        .iter()
        .position(|a| a == "--minimize")
        .and_then(|i| args.get(i + 1))
        .cloned();
    let numbers: Vec<u64> = args.iter().filter_map(|a| parse(a)).collect();
    let iterations = numbers.first().copied().unwrap_or(200);
    let seed = numbers.get(1).copied().unwrap_or(0x4A_0001);
    let authored = numbers.get(2).copied().unwrap_or(24) as usize;

    let report = modes::run(seed, iterations, authored);
    println!(
        "two-mode fuzz: {} histories from seed {seed:#x}, {authored} authored operations each",
        report.iterations
    );
    println!("{:<28} {:>9} {:>9}", "kind", "authored", "applied");
    for kind in modes::Coverage::kinds() {
        println!(
            "{kind:<28} {:>9} {:>9}",
            report.coverage.authored.get(&kind).copied().unwrap_or(0),
            report.coverage.applied.get(&kind).copied().unwrap_or(0)
        );
    }
    if args.iter().any(|a| a == "--effects") {
        println!("{:<28} {:>9}  effect", "kind", "count");
        for ((kind, shape), count) in &report.coverage.other {
            println!("{kind:<28} {count:>9}  {shape}");
        }
    }
    println!("findings: {}", report.findings.len());
    for (n, (class, (seed, history, finding))) in report.findings.iter().enumerate() {
        println!(
            "[{n}] {class}\n    seed {seed:#x}, {} envelopes\n    {}",
            history.len(),
            finding.detail.replace('\n', "\n    ")
        );
        if let Some(dir) = &minimize {
            let small = modes::minimize(history, class);
            let text = modes::render(
                &small,
                &[
                    format!("class: {class}"),
                    String::from("expect: split"),
                    format!(
                        "found: seed {seed:#x}, {authored} authored; minimized from {} to {} envelopes",
                        history.len(),
                        small.len()
                    ),
                ],
            );
            std::fs::create_dir_all(dir).expect("the directory");
            std::fs::write(format!("{dir}/{n}.txt"), text).expect("written");
            println!("    minimized to {} envelopes: {dir}/{n}.txt", small.len());
        }
    }
    if !report.findings.is_empty() {
        std::process::exit(1);
    }
}

fn parse(s: &str) -> Option<u64> {
    match s.strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16).ok(),
        None => s.parse().ok(),
    }
}
