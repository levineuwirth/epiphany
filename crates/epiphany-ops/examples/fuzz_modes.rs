//! The two-mode fuzz at a chosen budget (`epiphany_ops::fuzz::modes`):
//!
//!     cargo run --release -p epiphany-ops --example fuzz_modes -- \
//!         [ITERATIONS] [SEED] [AUTHORED] [--effects] [--minimize DIR]
//!     ... -- --recheck FILE...       # each committed history now, against its header
//!     ... -- --reminimize FILE...    # each shrunk again against its class, in place
//!
//! Prints each class of failure once with the first seed that showed it, and
//! how many of each kind were authored and applied (with `--effects`, what
//! became of the rest). With `--minimize DIR`,
//! writes each class's history, minimized, to `DIR/<n>.txt`. Exits nonzero
//! when anything was found.

use epiphany_ops::fuzz::modes;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--recheck") => return recheck(&args[1..]),
        Some("--reminimize") => return reminimize(&args[1..]),
        _ => {}
    }
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

/// The class and the `#` lines of a committed history.
fn header(text: &str) -> (String, Vec<&str>) {
    let class = text
        .lines()
        .find_map(|l| l.strip_prefix("# class: "))
        .expect("a `# class:` line")
        .to_owned();
    (class, text.lines().filter(|l| l.starts_with('#')).collect())
}

/// Prints, for each history, whether it now splits as its class, agrees, or
/// fails otherwise, beside what its header declares; exits nonzero where the
/// two disagree.
fn recheck(files: &[String]) {
    let mut disagree = 0;
    for path in files {
        let text = std::fs::read_to_string(path).expect("readable");
        let (class, _) = header(&text);
        // A deferred history still fails as its class.
        let declared_split = text
            .lines()
            .any(|l| l == "# expect: split" || l == "# expect: deferred");
        let found = modes::findings(&modes::parse(&text).expect("parses"));
        let now = if found.iter().any(|f| f.class == class) {
            "split"
        } else if found.is_empty() {
            "agree"
        } else {
            "other"
        };
        let matches = (now == "split") == declared_split && now != "other";
        disagree += usize::from(!matches);
        println!(
            "{} {now:<5} (declared {}) {class}{}",
            path.rsplit('/').next().unwrap_or(path),
            if declared_split { "split" } else { "agree" },
            if now == "other" {
                format!(
                    " | now: {:?}",
                    found.iter().map(|f| &f.class).collect::<Vec<_>>()
                )
            } else {
                String::new()
            }
        );
    }
    if disagree > 0 {
        std::process::exit(1);
    }
}

/// Shrinks each history again against its class, keeping its `#` lines.
fn reminimize(files: &[String]) {
    for path in files {
        let text = std::fs::read_to_string(path).expect("readable");
        let (class, lines) = header(&text);
        let history = modes::parse(&text).expect("parses");
        let small = modes::minimize(&history, &class);
        let mut out: String = lines.iter().map(|l| format!("{l}\n")).collect();
        out.push_str(&modes::render(&small, &[]));
        std::fs::write(path, out).expect("written");
        println!("{path}: {} -> {} envelopes", history.len(), small.len());
    }
}
