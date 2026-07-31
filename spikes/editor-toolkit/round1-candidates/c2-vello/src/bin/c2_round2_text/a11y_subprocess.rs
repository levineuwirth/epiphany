//! Check 5, `ReportPart::FixtureAndReportPlumbing`: the shared spike/report
//! harness around `a11y-verifier/verify.py` — verifier subprocesses, result
//! decoding and reduction, bus-unreachable evidence, and temporary/canonical
//! evidence-file handling.
//!
//! **Ruling (H1): this is harness, not product.** `a11y-verifier/verify.py`
//! exists once, is shared by both Round 2 candidates, and is not part of
//! either candidate's own accessibility stack — running it, trusting its
//! output only when the exit status and the JSON output actually agree, and
//! reducing five per-fixture outcomes to one round verdict is exactly the
//! kind of harness plumbing `ReportPart::FixtureAndReportPlumbing` is for,
//! not `AccessibilityIntegrationWiring`. An earlier revision of this packet
//! put all of this in the same file as the winit/`accesskit_winit` adapter
//! lifecycle (`a11y_wiring.rs`) — that file is now product-only; this one is
//! everything the ruling names as harness.
//!
//! ## F1/F2 — the two review findings this file's shape enforces
//!
//! **F1 (freshness).** Every fixture's `--json` output path is unique to
//! *this run* ([`run_nonce`], mixing pid + a timestamp) and is deleted
//! immediately before its `verify.py` invocation is spawned
//! ([`run_all_fixtures`]), so a read can never see anything this run did not
//! itself write. On top of that, [`interpret_verify_output`] cross-checks
//! the exit status against the JSON's own `verdict` field and its
//! `fixture_id` field, and refuses (hard error, never silently trusts
//! either) if they disagree.
//!
//! **F2 (ordering).** [`reduce_outcomes`] is a pure function over *every*
//! fixture's outcome, decided only after all five have been attempted — a
//! disqualifying `FAIL` found on any fixture always wins over an
//! environmental `BusUnreachable` found on another, in **either** order,
//! because [`run_all_fixtures`]'s loop never short-circuits on the first
//! `BusUnreachable`.
//!
//! ## H2 — the exit-2 conjunction, and why it is two separate conditions
//!
//! Exit 2 is only accepted as [`FixtureOutcome::BusUnreachable`] when
//! **both**, independently:
//!
//! - `inv.stdout` **begins with** [`CHECK5_NOT_RUN_PREFIX`] — the exact
//!   prefix `a11y-verifier/verify.py` prints for check 5's NOT RUN case,
//!   never for a usage error (which prints under a distinct `usage error`
//!   sentence instead — see that file's own `run_check5`);
//! - `inv.stdout` contains one of [`CHECK5_ENVIRONMENTAL_MARKERS`] —
//!   transcribed verbatim from `verify.py`'s own source, not guessed, with
//!   the exact call site named against each one.
//!
//! Both conditions are required, tested **separately** (each of the two
//! `interpret_exit_2_*_alone_is_a_hard_error` tests below holds the other
//! condition satisfied while breaking just the one it names), because a
//! conjunction whose two halves are only ever exercised together is not
//! actually verified — either half could be silently dropped and every
//! previously-passing test would keep passing.
//!
//! ## J2 — the worker thread and the readiness handoff moved here too
//!
//! [`run_a11y_round`] now owns spawning the worker thread that runs
//! [`run_all_fixtures`] and delivering its result — the *coordination*,
//! not the window. It drives `a11y_wiring::run_window`'s generic,
//! verifier-agnostic surface (`FinishHandle<Result<A11yRoundResult>>`): the
//! callback `run_window` invokes the instant the tree is actually live does
//! nothing but spawn a thread and return immediately, so the event loop
//! (product-side, `a11y_wiring.rs`) is never blocked by this file's work.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, bail, Result};

use round2_candidatekit::{A11yEvidence, BusUnreachableEvidence};

use crate::a11y_wiring::run_window;

/// Must match the binary target name (`c2_round2_text.rs` -> Cargo target
/// `c2_round2_text`): `accesskit_unix` (the AT-SPI2 bridge `accesskit_winit`
/// uses on Linux) derives the AT-SPI *application* name from
/// `std::env::current_exe()`'s file name (`accesskit_unix::context::app_name`,
/// verified in this workspace's lockfile at
/// `accesskit_unix-0.22.1/src/context.rs:36`), and
/// `a11y-verifier/verify.py`'s `--app-name` is a substring match against
/// that. **F4**: renamed from `round2_text` to `c2_round2_text` because both
/// candidate packages produced a binary named `round2_text`, colliding in
/// the shared `target/release/` output directory.
pub const APP_NAME: &str = "c2_round2_text";

/// The exact five fixture ids, recipe §2 order — restated (not read back out
/// of `round2-textkit`), the same discipline every crate in this packet
/// uses so a copy built independently still agrees on the roster.
pub const FIXTURE_ORDER: [&str; 5] = ["F-A", "F-B", "F-C", "F-D", "F-E"];

/// **H2**: the exact prefix `a11y-verifier/verify.py` prints for every
/// check-5 `NOT RUN` case — transcribed verbatim from that file (never
/// modified here, and never guessed): every `print(f"CHECK5: NOT RUN —
/// ...")` call, plus the one call site in `main()` that prints under a
/// dynamic `{label}` that is `"CHECK5"` in check-5 mode
/// (`a11y-verifier/verify.py:1092`), all begin with exactly this text.
/// `verify.py`'s usage-error branches (a bad `--expectations`, a digest
/// mismatch, an unknown `--fixture`, ...) print under a distinct `"CHECK5:
/// usage error — ..."` sentence instead and therefore never match this
/// prefix.
const CHECK5_NOT_RUN_PREFIX: &str = "CHECK5: NOT RUN";

/// **H2**: every environmental-cause marker `a11y-verifier/verify.py`
/// actually prints after [`CHECK5_NOT_RUN_PREFIX`], transcribed verbatim
/// from that file's source (never guessed), one entry per call site:
///
/// - `run_check5`, `Atspi.init()` raising: `verify.py:961`
///   (`f"CHECK5: NOT RUN — Atspi.init() failed: {exc}"`)
/// - `run_check5`, `Atspi.get_desktop(0)` raising: `verify.py:973`
///   (`f"CHECK5: NOT RUN — Atspi.get_desktop(0) failed: {exc}"`)
/// - `run_check5`, `Atspi.get_desktop(0)` returning `None`: `verify.py:976`
///   (`"CHECK5: NOT RUN — Atspi.get_desktop(0) returned None (no AT-SPI \
///   registry?)"`)
/// - `run_check5`, `desktop.get_child_count()` raising: `verify.py:984`
///   (`f"CHECK5: NOT RUN — desktop.get_child_count() failed: {exc}"`)
/// - `main`, the `gi.repository.Atspi` import itself failing — reached
///   *before* `run_check5` even starts: `verify.py:1092`
///   (`f"{label}: NOT RUN — could not import gi.repository.Atspi: {exc}"`,
///   `label == "CHECK5"` in check-5 mode)
///
/// Matched as a substring of `inv.stdout` *after* [`CHECK5_NOT_RUN_PREFIX`]
/// has already been confirmed present — the two checks are independent
/// (H2), so this list is consulted regardless of what precedes it in the
/// calling code, but the marker text itself never appears in any of
/// `verify.py`'s usage-error prints (`verify.py:934`, `:944`, `:953`), which
/// is what makes it a safe positive signal once the prefix is also
/// satisfied.
const CHECK5_ENVIRONMENTAL_MARKERS: &[&str] = &[
    "Atspi.init() failed",
    "Atspi.get_desktop(0) failed",
    "Atspi.get_desktop(0) returned None",
    "desktop.get_child_count() failed",
    "could not import gi.repository.Atspi",
];

/// The outcome of one full a11y round: either every fixture that could be
/// scored was, or the platform accessibility bus was found unreachable for
/// at least one fixture **and no fixture failed** — see [`reduce_outcomes`]
/// for why those two conditions must both hold (F2). `BusUnreachable` still
/// carries whatever fixtures *did* get scored before/around the bus issue
/// (`partial_scored`), so the report is not forced to discard real evidence
/// just because the round overall reads `NotRun`.
pub enum A11yRoundResult {
    Scored(Vec<A11yEvidence>),
    BusUnreachable {
        evidence: BusUnreachableEvidence,
        partial_scored: Vec<A11yEvidence>,
    },
}

/// One fixture's outcome, before the round-level F2 reduction.
#[derive(Debug)]
enum FixtureOutcome {
    Scored(A11yEvidence),
    BusUnreachable(BusUnreachableEvidence),
}

/// One `verify.py` invocation's raw result, in a form
/// [`interpret_verify_output`] can be exercised against without spawning a
/// subprocess (F1/H2's mutation tests).
struct VerifyInvocation {
    fixture_id: String,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
    /// The fresh, run-unique path this invocation was told to write its
    /// `--json` output to — already deleted (if anything occupied it)
    /// immediately before the subprocess was spawned. See this module's F1
    /// doc section.
    json_path: PathBuf,
}

fn evidence_from_json(fixture_id: &str, v: &serde_json::Value) -> Result<A11yEvidence> {
    let verdict = v
        .get("verdict")
        .and_then(|x| x.as_str())
        .ok_or_else(|| anyhow!("{fixture_id}: verify.py's json output is missing 'verdict'"))?;
    Ok(A11yEvidence {
        fixture_id: fixture_id.to_string(),
        platform: "at-spi2".to_string(),
        observed_name: v
            .get("observed_name")
            .and_then(|x| x.as_str())
            .map(str::to_string),
        observed_name_bytes_hex: v
            .get("observed_name_hex")
            .and_then(|x| x.as_str())
            .map(str::to_string),
        observed_role: v
            .get("observed_role")
            .and_then(|x| x.as_str())
            .map(str::to_string),
        prohibited_outcome: v
            .get("prohibited_outcome")
            .and_then(|x| x.as_str())
            .map(str::to_string),
        pass: verdict == "PASS",
        notes: v
            .get("reason")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string(),
    })
}

/// Interprets one already-completed `verify.py` invocation (F1, H2). Never
/// trusts a file's mere presence or a bare exit code alone:
///
/// - exit 0/1: reads `inv.json_path`, and requires **both** its `verdict`
///   field to agree with what the exit code implies (`0` -> `"PASS"`, `1`
///   -> `"FAIL"`) **and** its `fixture_id` field to equal `inv.fixture_id`.
///   Either disagreement is a hard error.
/// - exit 2: refuses to treat it as [`FixtureOutcome::BusUnreachable`] unless
///   **all three**, independently: `inv.json_path` is absent (a fresh scored
///   output existing alongside an exit-2 status is a contradiction, not
///   evidence); `inv.stdout` begins with [`CHECK5_NOT_RUN_PREFIX`]; and
///   `inv.stdout` contains one of [`CHECK5_ENVIRONMENTAL_MARKERS`] (H2).
///   Every other exit-2 shape (a usage error, a digest mismatch, ...) is a
///   hard error, never silently promoted to environmental absence.
fn interpret_verify_output(inv: &VerifyInvocation) -> Result<FixtureOutcome> {
    match inv.exit_code {
        Some(0) | Some(1) => {
            let code = inv.exit_code.expect("matched Some above");
            let expected_verdict = if code == 0 { "PASS" } else { "FAIL" };
            let text = std::fs::read_to_string(&inv.json_path).map_err(|e| {
                anyhow!(
                    "{}: verify.py exited {code} but its fresh --json output at {} could not be \
                     read: {e}\nstdout:\n{}",
                    inv.fixture_id,
                    inv.json_path.display(),
                    inv.stdout
                )
            })?;
            let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| {
                anyhow!(
                    "{}: failed to parse verify.py's json output: {e}",
                    inv.fixture_id
                )
            })?;
            let json_fixture_id =
                v.get("fixture_id")
                    .and_then(|x| x.as_str())
                    .ok_or_else(|| {
                        anyhow!(
                            "{}: verify.py's json output is missing 'fixture_id'",
                            inv.fixture_id
                        )
                    })?;
            if json_fixture_id != inv.fixture_id {
                bail!(
                    "{}: verify.py's --json output at {} names fixture_id {json_fixture_id:?}, \
                     not the fixture this invocation asked for — refusing to attribute someone \
                     else's verdict",
                    inv.fixture_id,
                    inv.json_path.display()
                );
            }
            let verdict = v.get("verdict").and_then(|x| x.as_str()).ok_or_else(|| {
                anyhow!(
                    "{}: verify.py's json output is missing 'verdict'",
                    inv.fixture_id
                )
            })?;
            if verdict != expected_verdict {
                bail!(
                    "{}: verify.py exited {code} (implying {expected_verdict:?}) but its own \
                     json output reports verdict {verdict:?} — exit status and json output \
                     disagree, refusing to trust either",
                    inv.fixture_id
                );
            }
            Ok(FixtureOutcome::Scored(evidence_from_json(
                &inv.fixture_id,
                &v,
            )?))
        }
        Some(2) => {
            if inv.json_path.exists() {
                bail!(
                    "{}: verify.py exited 2 (usage/NOT RUN) but a fresh --json output exists at \
                     {} anyway — an exit-2 run must never have written scored output, so this is \
                     a contradiction rather than evidence of anything",
                    inv.fixture_id,
                    inv.json_path.display()
                );
            }
            // H2: the two halves of the conjunction, computed and checked
            // independently -- see this module's doc comment for why they
            // must never be collapsed into one combined test of "looks
            // environmental".
            let has_prefix = inv.stdout.starts_with(CHECK5_NOT_RUN_PREFIX);
            let has_marker = CHECK5_ENVIRONMENTAL_MARKERS
                .iter()
                .any(|marker| inv.stdout.contains(marker));
            if has_prefix && has_marker {
                return Ok(FixtureOutcome::BusUnreachable(BusUnreachableEvidence {
                    probe_description: format!(
                        "python3 a11y-verifier/verify.py --fixture {} --app-name {APP_NAME} \
                         (AT-SPI2 client via gi.repository.Atspi)",
                        inv.fixture_id
                    ),
                    probe_output: inv.stdout.clone(),
                }));
            }
            bail!(
                "{}: verify.py exited 2 but stdout does not satisfy both required conditions \
                 (has_prefix={has_prefix}, has_marker={has_marker}) — treating as a hard usage \
                 error, not environmental NOT RUN: {}\n{}",
                inv.fixture_id,
                inv.stdout,
                inv.stderr
            );
        }
        other => bail!(
            "{}: verify.py exited with unexpected status {other:?}\nstdout:\n{}\nstderr:\n{}",
            inv.fixture_id,
            inv.stdout,
            inv.stderr
        ),
    }
}

/// Reduces every fixture's [`FixtureOutcome`] (collected in whatever order
/// they were attempted) to the round's overall result (F2).
///
/// A disqualifying `FAIL` found on **any** fixture always wins over a
/// `BusUnreachable` found on another — an environmental `NotRun` is only
/// admissible when **nothing failed**. The caller never short-circuits on
/// the first `BusUnreachable` (see [`run_all_fixtures`]), so both orderings
/// — a fail observed before a bus issue, or after one — reach this function
/// with the same two facts and therefore produce the same verdict.
fn reduce_outcomes(outcomes: Vec<FixtureOutcome>) -> A11yRoundResult {
    let mut scored = Vec::new();
    let mut bus_unreachable: Option<BusUnreachableEvidence> = None;
    for o in outcomes {
        match o {
            FixtureOutcome::Scored(ev) => scored.push(ev),
            FixtureOutcome::BusUnreachable(ev) => {
                if bus_unreachable.is_none() {
                    bus_unreachable = Some(ev);
                }
            }
        }
    }
    let any_fail = scored.iter().any(|e| !e.pass);
    match bus_unreachable {
        Some(evidence) if !any_fail => A11yRoundResult::BusUnreachable {
            evidence,
            partial_scored: scored,
        },
        _ => A11yRoundResult::Scored(scored),
    }
}

/// A per-process, per-call identifier mixed into every `--json` output path
/// this run creates (F1) — process id plus a nanosecond timestamp, cheap and
/// dependency-free. Not a cryptographic uniqueness guarantee by itself
/// (that is what the pre-spawn deletion in [`run_all_fixtures`] and the
/// exit-status/verdict/fixture-id cross-checks in
/// [`interpret_verify_output`] are for); it is the first line of defense,
/// making an accidental collision with another run's leftover file
/// vanishingly unlikely rather than structural.
fn run_nonce() -> String {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{pid}-{nanos}")
}

fn fresh_json_path(run_nonce: &str, fixture_id: &str) -> PathBuf {
    std::env::temp_dir().join(format!("c2-round2-a11y-{run_nonce}-{fixture_id}.json"))
}

/// Runs `a11y-verifier/verify.py` once per fixture, against the one live
/// window `a11y_wiring.rs` builds, out-of-process — the committed verifier
/// is the only thing that ever classifies a tree (task instructions:
/// "Scoring is not yours to decide. Run the committed verifier
/// out-of-process.").
///
/// **Never short-circuits (F2):** every fixture in [`FIXTURE_ORDER`] is
/// attempted regardless of what earlier fixtures returned, and the round's
/// overall verdict is decided once, by [`reduce_outcomes`], only after all
/// five outcomes are in hand.
fn run_all_fixtures(spike_root: &Path, digest: &str) -> Result<A11yRoundResult> {
    let verify_py = spike_root.join("a11y-verifier/verify.py");
    let expectations = spike_root.join("round2-a11y-oracle/a11y_expectations.json");
    let nonce = run_nonce();
    let mut outcomes = Vec::with_capacity(FIXTURE_ORDER.len());

    for fixture_id in FIXTURE_ORDER {
        let json_path = fresh_json_path(&nonce, fixture_id);
        // F1: never read a file this run did not write. Deleting whatever
        // (if anything) already occupies this path, immediately before
        // spawning, makes that a filesystem-level guarantee rather than
        // something inferred from the exit code alone.
        let _ = std::fs::remove_file(&json_path);

        let output = Command::new("python3")
            .arg(&verify_py)
            .arg("--expectations")
            .arg(&expectations)
            .arg("--fixture")
            .arg(fixture_id)
            .arg("--app-name")
            .arg(APP_NAME)
            .arg("--expect-source-digest")
            .arg(digest)
            .arg("--json")
            .arg(&json_path)
            .arg("--timeout")
            .arg("10")
            .current_dir(spike_root)
            .output()
            .map_err(|e| anyhow!("failed to spawn verify.py for {fixture_id}: {e}"))?;

        let inv = VerifyInvocation {
            fixture_id: fixture_id.to_string(),
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            json_path,
        };
        // A hard interpretation error (exit status/json disagreement, a
        // fixture-id mismatch, an unrecognized exit code, ...) is a harness
        // defect, not a legitimate outcome to defer judgement on — it
        // aborts the whole round immediately, unlike a `FAIL` or a
        // `BusUnreachable`, both of which are trustworthy typed outcomes
        // `reduce_outcomes` is free to weigh against each other.
        outcomes.push(interpret_verify_output(&inv)?);
    }

    Ok(reduce_outcomes(outcomes))
}

/// Opens the one probe window (`a11y_wiring::run_window`, product-side) and
/// scores all five fixtures against it (`run_all_fixtures`, this file),
/// then closes the window.
///
/// **J2**: this function, not `a11y_wiring.rs`, owns the coordination — the
/// worker thread, the fact that it runs `run_all_fixtures`, and delivering
/// the result. It drives `run_window`'s generic surface with `T =
/// Result<A11yRoundResult>`: the callback handed to `on_tree_published`
/// does nothing but spawn a thread and return immediately (never blocking
/// the event loop), and that thread's only two jobs are calling
/// `run_all_fixtures` and calling [`crate::a11y_wiring::FinishHandle::finish`]
/// with what it got.
///
/// `fixture_texts` must be in [`FIXTURE_ORDER`]'s order (F-A..F-E) — the
/// caller (`c2_round2_text.rs`) builds it directly from the loaded
/// `SpikeResolvedText::text` fields, never from a literal restated here, so
/// a fixture whose source string changed is exercised as it actually is.
pub fn run_a11y_round(
    spike_root: &Path,
    digest: &str,
    fixture_texts: [String; 5],
) -> Result<A11yRoundResult> {
    let spike_root = spike_root.to_path_buf();
    let digest = digest.to_string();
    run_window(fixture_texts, move |handle| {
        std::thread::spawn(move || {
            let result = run_all_fixtures(&spike_root, &digest);
            handle.finish(result);
        });
    })?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_json_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "c2-round2-a11y-subprocess-test-{name}-{}.json",
            std::process::id()
        ))
    }

    fn write_json(path: &Path, body: &serde_json::Value) {
        std::fs::write(path, serde_json::to_string_pretty(body).unwrap()).unwrap();
    }

    fn passing_body(fixture_id: &str) -> serde_json::Value {
        serde_json::json!({
            "fixture_id": fixture_id,
            "verdict": "PASS",
            "reason": "a node with an accepted role carries the accessible name byte-for-byte",
            "observed_role": "paragraph",
            "observed_name": "whatever",
            "observed_name_hex": "77686174657665",
            "prohibited_outcome": null,
            "walked_tree": []
        })
    }

    // ---- F1: freshness / exit-status-vs-json agreement ----

    /// Required kill (F1): a **stale** file at this run's json path claims
    /// `PASS`, but this invocation's exit code says `FAIL` (1) — the stale
    /// file must never be picked up as this fixture's evidence.
    #[test]
    fn interpret_rejects_a_stale_file_whose_verdict_disagrees_with_the_exit_code() {
        let path = scratch_json_path("stale-verdict");
        write_json(&path, &passing_body("F-A"));
        let inv = VerifyInvocation {
            fixture_id: "F-A".to_string(),
            exit_code: Some(1),
            stdout: "CHECK5: FAIL\n".to_string(),
            stderr: String::new(),
            json_path: path.clone(),
        };
        let err = interpret_verify_output(&inv).unwrap_err();
        assert!(err.to_string().contains("disagree"), "{err}");
        let _ = std::fs::remove_file(&path);
    }

    /// Required kill (F1): a fresh file that names a **different** fixture
    /// id must never be attributed to this one, even if exit code and
    /// verdict otherwise agree.
    #[test]
    fn interpret_rejects_a_json_whose_fixture_id_does_not_match() {
        let path = scratch_json_path("wrong-fixture-id");
        write_json(&path, &passing_body("F-B"));
        let inv = VerifyInvocation {
            fixture_id: "F-A".to_string(),
            exit_code: Some(0),
            stdout: "CHECK5: PASS\n".to_string(),
            stderr: String::new(),
            json_path: path.clone(),
        };
        let err = interpret_verify_output(&inv).unwrap_err();
        assert!(err.to_string().contains("not the fixture"), "{err}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn interpret_accepts_a_fresh_agreeing_pass() {
        let path = scratch_json_path("agreeing-pass");
        write_json(&path, &passing_body("F-A"));
        let inv = VerifyInvocation {
            fixture_id: "F-A".to_string(),
            exit_code: Some(0),
            stdout: "CHECK5: PASS\n".to_string(),
            stderr: String::new(),
            json_path: path.clone(),
        };
        let outcome = interpret_verify_output(&inv).unwrap();
        assert!(matches!(outcome, FixtureOutcome::Scored(e) if e.pass));
        let _ = std::fs::remove_file(&path);
    }

    /// Required kill (F1): exit 2 with generic usage-error stdout (no
    /// prefix, no marker) and **no** fresh output present must be a hard
    /// error, never silently promoted to bus-unreachable merely because
    /// there is nothing to read.
    #[test]
    fn interpret_exit_2_without_prefix_or_marker_and_no_fresh_output_is_a_hard_error() {
        let path = scratch_json_path("exit2-no-markers");
        let _ = std::fs::remove_file(&path); // guarantee absence
        let inv = VerifyInvocation {
            fixture_id: "F-A".to_string(),
            exit_code: Some(2),
            stdout: "CHECK5: usage error — bad --fixture value\n".to_string(),
            stderr: String::new(),
            json_path: path,
        };
        let err = interpret_verify_output(&inv).unwrap_err();
        assert!(err.to_string().contains("usage error"), "{err}");
    }

    #[test]
    fn interpret_exit_2_with_prefix_and_marker_and_no_fresh_output_is_bus_unreachable() {
        let path = scratch_json_path("exit2-with-markers");
        let _ = std::fs::remove_file(&path);
        let inv = VerifyInvocation {
            fixture_id: "F-A".to_string(),
            exit_code: Some(2),
            stdout: "CHECK5: NOT RUN — Atspi.init() failed: no bus\n".to_string(),
            stderr: String::new(),
            json_path: path,
        };
        let outcome = interpret_verify_output(&inv).unwrap();
        assert!(matches!(outcome, FixtureOutcome::BusUnreachable(_)));
    }

    /// **H2, required kill 1 of 2 (prefix present, marker absent).** stdout
    /// begins with the exact `CHECK5: NOT RUN` prefix, but names a cause
    /// this module does not recognise as environmental — must be a hard
    /// error. If the marker half of the conjunction were ever dropped
    /// (accept on prefix alone), this stdout would wrongly pass.
    #[test]
    fn interpret_exit_2_with_prefix_but_no_recognised_marker_is_a_hard_error() {
        let path = scratch_json_path("h2-prefix-no-marker");
        let _ = std::fs::remove_file(&path);
        let inv = VerifyInvocation {
            fixture_id: "F-A".to_string(),
            exit_code: Some(2),
            stdout: "CHECK5: NOT RUN — something we do not recognise\n".to_string(),
            stderr: String::new(),
            json_path: path,
        };
        let err = interpret_verify_output(&inv).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("hard usage error"), "{msg}");
        // The message must report *which half* was unmet: prefix true,
        // marker false — proof this is the marker-missing case specifically,
        // not a generic rejection.
        assert!(msg.contains("has_prefix=true"), "{msg}");
        assert!(msg.contains("has_marker=false"), "{msg}");
    }

    /// **H2, required kill 2 of 2 (marker present, prefix absent).** stdout
    /// contains a recognised environmental marker, but does not begin with
    /// the required `CHECK5: NOT RUN` prefix — must be a hard error. If the
    /// prefix half of the conjunction were ever dropped (accept on marker
    /// alone), this stdout would wrongly pass.
    #[test]
    fn interpret_exit_2_with_marker_but_no_prefix_is_a_hard_error() {
        let path = scratch_json_path("h2-marker-no-prefix");
        let _ = std::fs::remove_file(&path);
        let inv = VerifyInvocation {
            fixture_id: "F-A".to_string(),
            exit_code: Some(2),
            // A recognised marker string is present, but as a *substring*
            // of some other sentence, not as the required prefix.
            stdout: "some unrelated wrapper reported: Atspi.init() failed somewhere downstream\n"
                .to_string(),
            stderr: String::new(),
            json_path: path,
        };
        let err = interpret_verify_output(&inv).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("hard usage error"), "{msg}");
        // The message must report *which half* was unmet: marker true,
        // prefix false — proof this is the prefix-missing case specifically,
        // not a generic rejection.
        assert!(msg.contains("has_prefix=false"), "{msg}");
        assert!(msg.contains("has_marker=true"), "{msg}");
    }

    /// Required kill (F1): exit 2 with the required prefix and marker, but a
    /// fresh json output **exists anyway** — a contradiction (an exit-2 run
    /// must never have written scored output), so this must be a hard
    /// error, not accepted as bus-unreachable evidence.
    #[test]
    fn interpret_exit_2_with_prefix_and_marker_but_a_fresh_json_present_is_a_hard_error() {
        let path = scratch_json_path("exit2-contradiction");
        write_json(&path, &passing_body("F-A"));
        let inv = VerifyInvocation {
            fixture_id: "F-A".to_string(),
            exit_code: Some(2),
            stdout: "CHECK5: NOT RUN — Atspi.init() failed: no bus\n".to_string(),
            stderr: String::new(),
            json_path: path.clone(),
        };
        let err = interpret_verify_output(&inv).unwrap_err();
        assert!(err.to_string().contains("contradiction"), "{err}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn interpret_missing_json_on_exit_0_is_a_hard_error() {
        let path = scratch_json_path("missing-on-exit0");
        let _ = std::fs::remove_file(&path);
        let inv = VerifyInvocation {
            fixture_id: "F-A".to_string(),
            exit_code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
            json_path: path,
        };
        let err = interpret_verify_output(&inv).unwrap_err();
        assert!(err.to_string().contains("could not be read"), "{err}");
    }

    // ---- F2: a FAIL wins over a BusUnreachable found elsewhere, in either order ----

    fn fail_evidence(id: &str) -> A11yEvidence {
        A11yEvidence {
            fixture_id: id.to_string(),
            platform: "at-spi2".to_string(),
            observed_name: Some("".to_string()),
            observed_name_bytes_hex: Some("".to_string()),
            observed_role: None,
            prohibited_outcome: Some("absent-from-tree".to_string()),
            pass: false,
            notes: "no accessible-text-candidate node found".to_string(),
        }
    }

    fn pass_evidence(id: &str) -> A11yEvidence {
        A11yEvidence {
            fixture_id: id.to_string(),
            platform: "at-spi2".to_string(),
            observed_name: Some("x".to_string()),
            observed_name_bytes_hex: Some("78".to_string()),
            observed_role: Some("paragraph".to_string()),
            prohibited_outcome: None,
            pass: true,
            notes: "byte-for-byte".to_string(),
        }
    }

    fn some_bus_evidence() -> BusUnreachableEvidence {
        BusUnreachableEvidence {
            probe_description: "test probe".to_string(),
            probe_output: "CHECK5: NOT RUN — Atspi.init() failed: no bus".to_string(),
        }
    }

    /// Required kill (F2): a FAIL observed before a bus-unreachable outcome
    /// still disqualifies — the round must not report NotRun.
    #[test]
    fn a_fail_before_a_bus_unreachable_still_wins() {
        let outcomes = vec![
            FixtureOutcome::Scored(fail_evidence("F-A")),
            FixtureOutcome::BusUnreachable(some_bus_evidence()),
        ];
        let result = reduce_outcomes(outcomes);
        match result {
            A11yRoundResult::Scored(evidence) => {
                assert!(evidence.iter().any(|e| !e.pass), "the FAIL must survive");
            }
            A11yRoundResult::BusUnreachable { .. } => {
                panic!("a FAIL found anywhere must never be erased by a later BusUnreachable")
            }
        }
    }

    /// Required kill (F2), the other ordering: a bus-unreachable observed
    /// **before** a FAIL must reach the exact same verdict as the previous
    /// test — ordering must never decide a disqualifying check.
    #[test]
    fn a_bus_unreachable_before_a_fail_still_loses_to_the_fail() {
        let outcomes = vec![
            FixtureOutcome::BusUnreachable(some_bus_evidence()),
            FixtureOutcome::Scored(fail_evidence("F-C")),
        ];
        let result = reduce_outcomes(outcomes);
        match result {
            A11yRoundResult::Scored(evidence) => {
                assert!(evidence.iter().any(|e| !e.pass), "the FAIL must survive");
            }
            A11yRoundResult::BusUnreachable { .. } => {
                panic!(
                    "the FAIL must win regardless of whether the BusUnreachable was observed \
                     before or after it"
                )
            }
        }
    }

    /// A bus-unreachable with **no** FAIL anywhere is the legitimate
    /// environmental-absence case — this is the one place `BusUnreachable`
    /// is allowed to be the verdict.
    #[test]
    fn a_bus_unreachable_with_no_fail_anywhere_is_not_run() {
        let outcomes = vec![
            FixtureOutcome::Scored(pass_evidence("F-A")),
            FixtureOutcome::BusUnreachable(some_bus_evidence()),
            FixtureOutcome::Scored(pass_evidence("F-D")),
        ];
        let result = reduce_outcomes(outcomes);
        assert!(matches!(result, A11yRoundResult::BusUnreachable { .. }));
    }

    #[test]
    fn all_pass_and_no_bus_issue_is_scored() {
        let outcomes = vec![
            FixtureOutcome::Scored(pass_evidence("F-A")),
            FixtureOutcome::Scored(pass_evidence("F-B")),
        ];
        let result = reduce_outcomes(outcomes);
        match result {
            A11yRoundResult::Scored(evidence) => assert_eq!(evidence.len(), 2),
            A11yRoundResult::BusUnreachable { .. } => panic!("no bus issue was reported"),
        }
    }
}
