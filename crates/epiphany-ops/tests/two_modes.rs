//! The two-mode fuzz in CI (`epiphany_ops::fuzz::modes`), and every history
//! it has found failing, minimized and committed under `tests/two_modes/`.
//!
//! A committed history is one envelope per line in the envelope text form,
//! after `#` lines. Two of them are read here: `# class:` names the failure
//! the fuzz found (an effect split, the objects or canonical bytes differing,
//! or an invariant the graph-aware score breaks), and `# expect:` says
//! whether the history must now reduce alike (`agree`) or still shows that
//! failure (`split`), for a failure not yet fixed. A fix flips its histories
//! from `split` to `agree`; a `split` history whose failure has gone fails
//! here until it is flipped, so a fix cannot pass unrecorded. A failure the
//! owner has deferred is `# expect: deferred`, with a `# deferred:` line
//! giving the reason: its history is kept and run by an ignored test, which
//! requires it to agree, so `cargo test -- --ignored` shows it failing until
//! the fix lands.
//!
//! The CI budget runs a small number of generated histories and requires
//! that every failure it finds is a committed `split` or `deferred` class, and
//! that every operation kind and payload is authored and applied. The local
//! budget is the `fuzz_modes` example.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::OnceLock;

use epiphany_ops::fuzz::modes;

/// The CI budget: histories, their first seed, and the operations each
/// authors after its genesis.
const CI_HISTORIES: u64 = 96;
const CI_SEED: u64 = 0x4A_0001;
const CI_AUTHORED: usize = 24;

/// The CI budget's run, shared by the tests that read it.
fn ci_run() -> &'static modes::Report {
    static REPORT: OnceLock<modes::Report> = OnceLock::new();
    REPORT.get_or_init(|| modes::run(CI_SEED, CI_HISTORIES, CI_AUTHORED))
}

/// What a committed history declares.
#[derive(PartialEq)]
enum Expect {
    Agree,
    Split,
    /// Deferred by the owner, with the reason.
    Deferred(String),
}

struct Committed {
    file: String,
    class: String,
    expect: Expect,
    history: Vec<epiphany_ops::OperationEnvelope>,
}

fn committed() -> Vec<Committed> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/two_modes");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("the committed histories' directory")
        .map(|e| e.expect("a directory entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "txt"))
        .collect();
    files.sort();
    files
        .into_iter()
        .map(|path| {
            let file = path.file_name().unwrap().to_string_lossy().into_owned();
            let text = std::fs::read_to_string(&path).expect("readable");
            let header = |key: &str| {
                text.lines()
                    .find_map(|l| l.strip_prefix(&format!("# {key}: ")))
                    .unwrap_or_else(|| panic!("{file} has no `# {key}:` line"))
                    .to_owned()
            };
            let class = header("class");
            let expect = match header("expect").as_str() {
                "split" => Expect::Split,
                "agree" => Expect::Agree,
                "deferred" => Expect::Deferred(header("deferred")),
                other => panic!("{file}: `# expect: {other}` is not split, agree or deferred"),
            };
            let history = modes::parse(&text).unwrap_or_else(|e| panic!("{file}: {e:?}"));
            assert!(!history.is_empty(), "{file} holds no envelope");
            Committed {
                file,
                class,
                expect,
                history,
            }
        })
        .collect()
}

#[test]
fn every_committed_history_reduces_as_it_declares() {
    let all = committed();
    assert!(!all.is_empty(), "no committed history was read");
    for c in &all {
        assert!(
            modes::valid(&c.history),
            "{}: names an object no operation its author saw minted",
            c.file
        );
        let found = modes::findings(&c.history);
        if matches!(c.expect, Expect::Deferred(_)) {
            continue;
        }
        if c.expect == Expect::Split {
            assert!(
                found.iter().any(|f| f.class == c.class),
                "{}: declared to split as `{}`, but finds {:?}; if the fix is in, \
                 flip it to `# expect: agree`",
                c.file,
                c.class,
                found.iter().map(|f| &f.class).collect::<Vec<_>>()
            );
        } else {
            assert!(
                found.is_empty(),
                "{}: declared to agree, but finds {:#?}",
                c.file,
                found
            );
        }
    }
}

/// Concurrent region creation at one place is deferred (owner's ruling D48):
/// two authors each create a region over the same time and staves, and
/// refusing the second needs the regions' time extents compared in both
/// modes, which needs anchors resolved in base-free reduction. Wanted when
/// collaboration arrives; until then the history is kept and run here.
#[test]
#[ignore = "concurrent region creation at one place is deferred (D48): refusing \
            the second needs region time extents resolved in base-free reduction"]
fn every_deferred_history_reduces_alike() {
    let deferred: Vec<Committed> = committed()
        .into_iter()
        .filter(|c| matches!(c.expect, Expect::Deferred(_)))
        .collect();
    assert!(!deferred.is_empty(), "no deferred history was read");
    for c in &deferred {
        let found = modes::findings(&c.history);
        assert!(
            found.is_empty(),
            "{} (deferred: {}): finds {:#?}",
            c.file,
            match &c.expect {
                Expect::Deferred(reason) => reason.as_str(),
                _ => unreachable!(),
            },
            found
        );
    }
}

#[test]
fn the_ci_budget_finds_no_failure_that_is_not_committed() {
    let known: BTreeSet<String> = committed()
        .into_iter()
        .filter(|c| c.expect != Expect::Agree)
        .map(|c| c.class)
        .collect();
    let report = ci_run();
    let unknown: Vec<_> = report
        .findings
        .iter()
        .filter(|(class, _)| !known.contains(*class))
        .map(|(class, (seed, history, finding))| {
            format!(
                "{class}\n  seed {seed:#x}, {} envelopes; minimize with the fuzz_modes \
                 example and commit it\n  {}",
                history.len(),
                finding.detail
            )
        })
        .collect();
    assert!(unknown.is_empty(), "{}", unknown.join("\n"));
}

#[test]
fn the_ci_budget_authors_and_applies_every_kind() {
    let report = ci_run();
    let mut missing = Vec::new();
    for kind in modes::Coverage::kinds() {
        let authored = report.coverage.authored.get(&kind).copied().unwrap_or(0);
        let applied = report.coverage.applied.get(&kind).copied().unwrap_or(0);
        if authored == 0 || applied == 0 {
            missing.push(format!("{kind}: authored {authored}, applied {applied}"));
        }
    }
    assert!(missing.is_empty(), "{missing:#?}");
}

/// The deferred cause is named apart from the invariant's others, so the
/// exception covers it alone: the deferred history, its second region's
/// author made to have seen the first region's create, still overlaps them,
/// and is classed `RegionExtents` plainly, which nothing excepts.
#[test]
fn a_region_overlap_its_author_saw_is_not_the_deferred_class() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/two_modes/110-invariant-region-extents.txt");
    let text = std::fs::read_to_string(path).expect("readable");
    let unaware =
        "(stamp 37 0 #x00000000000000010000000000000003) (causal ((#x0000000000000001 2)) ())";
    assert_eq!(
        text.matches(unaware).count(),
        1,
        "the second region's create"
    );
    let aware = text.replace(
        unaware,
        "(stamp 37 0 #x00000000000000010000000000000003) \
         (causal ((#x0000000000000001 2) (#x0000000000000002 0)) ())",
    );
    let history = modes::parse(&aware).expect("parses");
    assert!(modes::valid(&history));
    let classes: Vec<String> = modes::findings(&history)
        .into_iter()
        .map(|f| f.class)
        .collect();
    assert!(
        classes
            .iter()
            .any(|c| c == "invariant Invariant(RegionExtents"),
        "{classes:?}"
    );
    assert!(
        classes.iter().all(|c| c != modes::CONCURRENT_REGIONS),
        "{classes:?}"
    );
    let unchanged = modes::findings(&modes::parse(&text).expect("parses"));
    assert!(unchanged
        .iter()
        .any(|f| f.class == modes::CONCURRENT_REGIONS));
}
