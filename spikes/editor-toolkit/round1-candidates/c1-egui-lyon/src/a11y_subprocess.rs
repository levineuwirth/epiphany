//! `ReportPart::AccessibilityIntegrationWiring`, half two — "the subprocess
//! orchestration of the verifier" (the F3 fix's own definition of this row,
//! naming this exact responsibility). `a11y_app.rs` is the other half (the
//! window/event-loop/adapter-lifecycle side).
//!
//! This module spawns `bin/c1_round2_a11y.rs` once per fixture, runs
//! `a11y-verifier/verify.py` out-of-process against the live AT-SPI2 bus,
//! and turns its exit status + `--json` output into a
//! [`FixtureCheck5Outcome`] — never a same-process self-report.
//!
//! ## F1 — freshness
//!
//! Every invocation writes to a **fresh, unique, run-private scratch path**
//! ([`fresh_temp_path`]: candidate PID + a monotonic counter + a nanosecond
//! timestamp, in the system temp directory) that cannot have existed before
//! this call, so [`interpret_verify_output`] can never read a file this run
//! did not write — the earlier revision of this file used one fixed path
//! per fixture (`round2_a11y_evidence/<ID>.json`) and checked
//! `json_path.exists()` *before* looking at the exit status, so a stale
//! file from a previous run could be read back as if it were this run's
//! own output, and a usage
//! error's exit 2 with no file present was treated identically to a
//! genuine bus-unreachable NOT RUN. [`interpret_verify_output`] now
//! requires the exit status and the JSON verdict to **agree** (disagreement
//! is a hard [`anyhow::Error`], never a guess) and validates the JSON's own
//! `fixture_id` field against the fixture actually requested.
//!
//! ## G1 — an allow-list, not a deny-list
//!
//! F1's first cut still admitted almost anything at exit 2 as bus-
//! unreachable evidence: it rejected only stdout *beginning* with
//! `"CHECK5: usage error"` and treated everything else — empty stdout, an
//! unrecognised message, a stderr-only argparse failure (which leaves
//! stdout empty) — as NOT RUN. [`interpret_verify_output`] now **requires**
//! the exact `"CHECK5: NOT RUN"` prefix **and** one of
//! [`APPROVED_NOT_RUN_MARKERS`], read directly from `verify.py`'s own exit
//! points rather than guessed. Everything else at exit 2 is a hard `Err`.
//!
//! ## F2 — ordering independence
//!
//! [`aggregate_check5`] checks for **any** `Fail` first, over the whole
//! fixture set, before it ever looks at whether a `NotRun` also occurred —
//! the earlier revision let whichever outcome was seen *last* in the loop
//! decide, so a FAIL on one fixture followed by a legitimate NOT RUN on
//! another silently discarded the FAIL. A FAIL is disqualifying in
//! **either** ordering; an environmental NOT RUN can only apply when
//! nothing failed.
//!
//! ## G3 — publish, don't accumulate
//!
//! F1's freshness fix put the per-run private path inside `evidence_dir`
//! itself, so every run left its own PID/timestamp-named file behind and
//! the directory grew without bound (ten files after two runs of five
//! fixtures, never cleaned). *Validating* freshness and *publishing*
//! evidence are now two separate steps: [`fresh_temp_path`] writes into the
//! system temp directory (never `evidence_dir`), [`interpret_verify_output`]
//! validates it there, and only a validated outcome is
//! [`publish_canonical`]-ed to `evidence_dir`'s canonical `<fixture_id>.json`
//! — overwriting any earlier run's file, never adding to it, and never
//! itself read back by anything in this module.

use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};

use round2_candidatekit::{scoring, A11yEvidence, BusUnreachableEvidence, CheckOutcome};

/// Must match `bin/c1_round2_a11y.rs`'s built binary name (the file's own
/// stem — Cargo auto-names a `src/bin/*.rs` target after its file, and this
/// crate declares no `[[bin]] name` override). This is what
/// `a11y-verifier/verify.py --app-name` filters against, since AT-SPI's own
/// application name tracks the process/binary name, not the `eframe`
/// window title (measured in `round0-evidence/c1-egui-readback.txt`).
///
/// F4: renamed from `round2_a11y` to `c1_round2_a11y` — both Round 2
/// candidates had built a binary literally named `round2_text`/`round2_a11y`,
/// which collided in the shared `target/` output directory (a locked
/// release build can hold only one binary per name; C2's silently won).
pub const A11Y_APP_NAME: &str = "c1_round2_a11y";

/// One fixture's check-5 outcome, as actually observed — never inferred
/// from an absent file or a convenient default.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FixtureCheck5Outcome {
    Pass {
        observed_role: Option<String>,
        observed_name: Option<String>,
    },
    Fail {
        reason: String,
        observed_role: Option<String>,
        observed_name: Option<String>,
        prohibited_outcome: Option<String>,
    },
    /// Admissible **only** when verify.py itself reported the AT-SPI bus
    /// unreachable: the exact `"CHECK5: NOT RUN"` prefix **and** one of
    /// [`APPROVED_NOT_RUN_MARKERS`] (G1). Never constructed for a usage
    /// error on this candidate's own invocation, empty stdout, unrecognised
    /// stdout, or a stderr-only failure — all of those are a hard `Err`
    /// from [`interpret_verify_output`] instead.
    NotRun { reason: String },
}

/// **G1.** Every substring `a11y-verifier/verify.py` itself prints
/// immediately after its `"CHECK5: NOT RUN — "` prefix, at every exit point
/// that reaches `sys.exit(2)` for a genuine environmental cause (read
/// directly from `verify.py`, not guessed):
///
/// - `"could not import gi.repository.Atspi"` — `main()`, before dispatch
/// - `"Atspi.init() failed"` — `run_check5`
/// - `"Atspi.get_desktop(0) failed"` — `run_check5`
/// - `"Atspi.get_desktop(0) returned None"` — `run_check5`
/// - `"desktop.get_child_count() failed"` — `run_check5`
///
/// `verify.py` also exits 2 for three **usage** errors (`"CHECK5: usage
/// error — "`, not this prefix at all), which are refused below regardless
/// of marker. G1's fix is that admission now requires **both** the exact
/// `"CHECK5: NOT RUN"` prefix **and** one of these markers — empty stdout,
/// unrecognised stdout, and a stderr-only failure (argparse writes to
/// stderr, leaving stdout empty) all fail every one of these checks and are
/// refused, where the previous revision admitted all three as bus-
/// unreachable evidence merely because they did not start with
/// `"CHECK5: usage error"`.
const APPROVED_NOT_RUN_MARKERS: &[&str] = &[
    "could not import gi.repository.Atspi",
    "Atspi.init() failed",
    "Atspi.get_desktop(0) failed",
    "Atspi.get_desktop(0) returned None",
    "desktop.get_child_count() failed",
];

static EVIDENCE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// **G3.** A run-private **scratch** path that cannot have existed before
/// this call: the system temp directory (never `evidence_dir` — see
/// [`publish_canonical`]'s doc comment for why the two must be different
/// directories) + process id + a monotonically increasing in-process
/// counter + a nanosecond timestamp, none of which repeat across
/// invocations of this binary. F1: this is what makes "never read a file
/// this run did not write" true *structurally*, rather than by convention —
/// there is no other writer that could have created a file at this exact
/// path first. [`run_check5_for_fixture`] removes this file again once it
/// has been validated and (if applicable) published, so it never
/// accumulates.
fn fresh_temp_path(fixture_id: &str) -> PathBuf {
    let pid = std::process::id();
    let seq = EVIDENCE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "c1-round2-a11y-{fixture_id}-{pid}-{seq}-{nanos}.json"
    ))
}

/// **G3: publish, never read back.** Copies the already-validated scratch
/// JSON at `temp_path` to `evidence_dir`'s **canonical** name for
/// `fixture_id` (`<fixture_id>.json`) — overwriting whatever an earlier run
/// left there — for a `Pass`/`Fail` outcome; for a `NotRun` outcome there is
/// no fresh JSON at all (verify.py never writes one at exit 2), so any
/// stale canonical file from an earlier run is removed instead, so the
/// directory always reflects only this run's own evidence.
///
/// **This function is write-only with respect to `evidence_dir`.** Nothing
/// in this module ever opens a file under `evidence_dir` to decide
/// anything — every decision ([`interpret_verify_output`]) is made from the
/// private scratch path before this function is ever called. A canonical
/// file is a **report artifact**, not an input: keeping the two directories
/// distinct (scratch in the system temp dir, published in `evidence_dir`)
/// makes "the run must still refuse to read a file it did not write this
/// run" true by construction — there is no code path that reads from
/// `evidence_dir` at all.
///
/// Idempotent: calling this once per fixture per run, for a fixed
/// five-fixture set, always leaves exactly five files in `evidence_dir` —
/// never a growing pile of timestamped names (the defect this fix closes;
/// see `publishing_is_idempotent_two_runs_leave_exactly_five_files`).
fn publish_canonical(
    evidence_dir: &Path,
    fixture_id: &str,
    temp_path: &Path,
    outcome: &FixtureCheck5Outcome,
) -> Result<()> {
    let canonical = evidence_dir.join(format!("{fixture_id}.json"));
    match outcome {
        FixtureCheck5Outcome::Pass { .. } | FixtureCheck5Outcome::Fail { .. } => {
            std::fs::copy(temp_path, &canonical).with_context(|| {
                format!(
                    "failed to publish {} -> {}",
                    temp_path.display(),
                    canonical.display()
                )
            })?;
        }
        FixtureCheck5Outcome::NotRun { .. } => {
            // No fresh JSON exists for this fixture this run; do not leave
            // a stale one behind claiming otherwise.
            let _ = std::fs::remove_file(&canonical);
        }
    }
    Ok(())
}

/// Turns one `verify.py` invocation's exit status + stdout + (possibly
/// absent) fresh `--json` output into a [`FixtureCheck5Outcome`], or a hard
/// [`anyhow::Error`] when the two disagree, when the JSON's own
/// `fixture_id` does not match what was requested, or when a file exists
/// where the exit status says none should.
///
/// **This function never trusts the file alone, and never trusts the exit
/// code alone** — F1's whole point. `json_path` must be a path this
/// specific invocation was told to write to ([`fresh_temp_path`]); a
/// caller must never pass a path some earlier run might have written, and
/// this function never reads from `evidence_dir` — see [`publish_canonical`]
/// (G3).
fn interpret_verify_output(
    json_path: &Path,
    code: Option<i32>,
    fixture_id: &str,
    stdout: &str,
) -> Result<FixtureCheck5Outcome> {
    let exists = json_path.exists();

    match code {
        Some(0) | Some(1) => {
            if !exists {
                return Err(anyhow!(
                    "verify.py exited {code:?} for {fixture_id} but wrote no JSON at {} — exit \
                     status and output must agree; refusing to guess",
                    json_path.display()
                ));
            }
            let text = std::fs::read_to_string(json_path)
                .with_context(|| format!("failed to read {}", json_path.display()))?;
            let v: serde_json::Value = serde_json::from_str(&text)
                .with_context(|| format!("failed to parse {}", json_path.display()))?;
            let json_fixture_id = v["fixture_id"].as_str().unwrap_or("");
            if json_fixture_id != fixture_id {
                return Err(anyhow!(
                    "verify.py's JSON at {} reports fixture_id {json_fixture_id:?}, but this \
                     invocation asked for {fixture_id:?} — refusing to attribute someone else's \
                     result",
                    json_path.display()
                ));
            }
            let verdict = v["verdict"].as_str().unwrap_or("");
            let expected = if code == Some(0) { "PASS" } else { "FAIL" };
            if verdict != expected {
                return Err(anyhow!(
                    "verify.py exited {code:?} (implying {expected}) for {fixture_id}, but its \
                     own JSON verdict is {verdict:?} — exit status and JSON output disagree, \
                     which must never be resolved by trusting either one silently"
                ));
            }
            let reason = v["reason"].as_str().unwrap_or("").to_string();
            let observed_role = v["observed_role"].as_str().map(|s| s.to_string());
            let observed_name = v["observed_name"].as_str().map(|s| s.to_string());
            let prohibited_outcome = v["prohibited_outcome"].as_str().map(|s| s.to_string());
            if verdict == "PASS" {
                Ok(FixtureCheck5Outcome::Pass {
                    observed_role,
                    observed_name,
                })
            } else {
                Ok(FixtureCheck5Outcome::Fail {
                    reason,
                    observed_role,
                    observed_name,
                    prohibited_outcome,
                })
            }
        }
        Some(2) => {
            // F1: a file existing here at all is a contradiction — exit 2
            // means verify.py's own `run_check5` returned before ever
            // reaching its `--json` write (every usage-error and NOT-RUN
            // exit point in verify.py precedes that write). A file present
            // anyway (a stale leftover, a path collision) is refused rather
            // than read, which is exactly the "stale PASS file" failure
            // mode F1 exists to close.
            if exists {
                return Err(anyhow!(
                    "verify.py exited 2 (usage error / NOT RUN) for {fixture_id}, but a JSON \
                     file exists at {} anyway — verify.py's own contract is that exit 2 never \
                     writes --json, so this is refused rather than read as if it were fresh",
                    json_path.display()
                ));
            }
            // G1: an **allow-list**, not a deny-list. Admission requires the
            // exact "CHECK5: NOT RUN" prefix AND one of
            // APPROVED_NOT_RUN_MARKERS naming the actual environmental
            // cause — not merely "did not say usage error". Empty stdout,
            // unrecognised stdout, and a stderr-only argparse failure (which
            // leaves stdout empty) all fail this and are refused below,
            // exactly the "almost anything qualifies" failure mode G1
            // exists to close.
            let first_line = stdout.lines().next().unwrap_or("");
            let has_marker = APPROVED_NOT_RUN_MARKERS
                .iter()
                .any(|m| first_line.contains(m));
            if first_line.starts_with("CHECK5: NOT RUN") && has_marker {
                Ok(FixtureCheck5Outcome::NotRun {
                    reason: stdout.to_string(),
                })
            } else {
                // Everything else at exit 2: a usage error on THIS
                // candidate's own invocation, empty stdout, unrecognised
                // stdout, or a stderr-only failure — never an environmental
                // absence, and must not become admissible bus-unreachable
                // evidence.
                Err(anyhow!(
                    "verify.py exited 2 for {fixture_id} without an approved NOT-RUN marker — \
                     refusing to admit this as bus-unreachable evidence. stdout={stdout:?}"
                ))
            }
        }
        other => Err(anyhow!(
            "verify.py exited with unexpected status {other:?} for {fixture_id}: {stdout}"
        )),
    }
}

/// Spawns `bin/c1_round2_a11y.rs` for `fixture_id`, runs `verify.py` against
/// it into a run-private [`fresh_temp_path`], kills the app, validates the
/// result via [`interpret_verify_output`], **then** publishes it to
/// `evidence_dir` via [`publish_canonical`] (G3) — validation and
/// publishing are two separate steps, in that order, so a rejected result
/// is never published.
fn run_check5_for_fixture(
    root: &Path,
    a11y_bin: &Path,
    evidence_dir: &Path,
    fixture_id: &str,
) -> Result<FixtureCheck5Outcome> {
    let mut child: Child = Command::new(a11y_bin)
        .arg("--fixture")
        .arg(fixture_id)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .with_context(|| format!("failed to spawn {}", a11y_bin.display()))?;

    let temp_path = fresh_temp_path(fixture_id);

    let expectations = root.join("round2-a11y-oracle/a11y_expectations.json");
    let verify_py = root.join("a11y-verifier/verify.py");
    let digest = round2_textkit::output::expected_artifact_digest();

    let output = Command::new("python3")
        .arg(&verify_py)
        .arg("--expectations")
        .arg(&expectations)
        .arg("--fixture")
        .arg(fixture_id)
        .arg("--app-name")
        .arg(A11Y_APP_NAME)
        .arg("--expect-source-digest")
        .arg(digest)
        .arg("--json")
        .arg(&temp_path)
        .arg("--timeout")
        .arg("15")
        .output();

    // Always try to kill the app, whatever verify.py did.
    let _ = child.kill();
    let _ = child.wait();

    let output = output.context("failed to invoke a11y-verifier/verify.py")?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let code = output.status.code();

    let outcome = interpret_verify_output(&temp_path, code, fixture_id, &stdout);
    // Publish only on a validated outcome — never a rejected one — and
    // clean up the scratch file regardless, so it never accumulates.
    let result = match &outcome {
        Ok(o) => publish_canonical(evidence_dir, fixture_id, &temp_path, o),
        Err(_) => Ok(()),
    };
    let _ = std::fs::remove_file(&temp_path);
    result?;
    outcome
}

/// Reduces every fixture's [`FixtureCheck5Outcome`] to the report's
/// `check5_accessibility` cell + (when applicable)
/// [`BusUnreachableEvidence`].
///
/// **F2: any `Fail`, anywhere in `outcomes`, wins over any `NotRun`,
/// regardless of which was observed first.** An environmental `NotRun` is
/// admissible only when *nothing* failed.
fn aggregate_check5(
    outcomes: &[(String, FixtureCheck5Outcome)],
) -> (CheckOutcome, Option<BusUnreachableEvidence>) {
    let failing: Vec<&str> = outcomes
        .iter()
        .filter(|(_, o)| matches!(o, FixtureCheck5Outcome::Fail { .. }))
        .map(|(id, _)| id.as_str())
        .collect();
    if !failing.is_empty() {
        return (
            CheckOutcome::fail(format!(
                "{}/{} fixtures failed check 5: {} — see a11y_evidence for the exact \
                 prohibited_outcome per fixture",
                failing.len(),
                outcomes.len(),
                failing.join(", ")
            ))
            .expect("non-empty reason"),
            None,
        );
    }

    if let Some((id, reason)) = outcomes.iter().find_map(|(id, o)| match o {
        FixtureCheck5Outcome::NotRun { reason } => Some((id.clone(), reason.clone())),
        _ => None,
    }) {
        return (
            CheckOutcome::not_run(format!(
                "{id}: verify.py reported the AT-SPI2 bus unreachable: {reason}"
            ))
            .expect("non-empty reason"),
            Some(BusUnreachableEvidence {
                probe_description: format!(
                    "a11y-verifier/verify.py --expectations ... --fixture {id} --app-name \
                     {A11Y_APP_NAME} (gi.repository.Atspi, connected to the live AT-SPI2 session \
                     bus)"
                ),
                probe_output: reason,
            }),
        );
    }

    (CheckOutcome::Pass, None)
}

/// The whole of check 5: `check5_accessibility`, its bus-unreachable
/// evidence (if any), and every fixture's recorded [`A11yEvidence`].
pub struct Check5Result {
    pub check5_accessibility: CheckOutcome,
    pub check5_bus_unreachable_evidence: Option<BusUnreachableEvidence>,
    pub a11y_evidence: Vec<A11yEvidence>,
}

/// Runs check 5 for every fixture in `fixture_ids`, in order, printing
/// progress as it goes.
pub fn run_all(
    root: &Path,
    a11y_bin: &Path,
    evidence_dir: &Path,
    fixture_ids: &[String],
) -> Result<Check5Result> {
    std::fs::create_dir_all(evidence_dir)
        .with_context(|| format!("failed to create {}", evidence_dir.display()))?;
    let mut outcomes = Vec::with_capacity(fixture_ids.len());
    for fixture_id in fixture_ids {
        println!(
            "check 5: running {} for {fixture_id}...",
            a11y_bin.display()
        );
        let outcome = run_check5_for_fixture(root, a11y_bin, evidence_dir, fixture_id)?;
        println!("  {fixture_id} -> {outcome:?}");
        outcomes.push((fixture_id.clone(), outcome));
    }

    let a11y_evidence = outcomes
        .iter()
        .map(|(fixture_id, outcome)| match outcome {
            FixtureCheck5Outcome::Pass {
                observed_role,
                observed_name,
            } => A11yEvidence {
                fixture_id: fixture_id.clone(),
                platform: scoring::ROUND_PLATFORM.to_string(),
                observed_name: observed_name.clone(),
                observed_name_bytes_hex: observed_name
                    .as_ref()
                    .map(|s| s.as_bytes().iter().map(|b| format!("{b:02x}")).collect()),
                observed_role: observed_role.clone(),
                prohibited_outcome: None,
                pass: true,
                notes: "PASS".to_string(),
            },
            FixtureCheck5Outcome::Fail {
                reason,
                observed_role,
                observed_name,
                prohibited_outcome,
            } => A11yEvidence {
                fixture_id: fixture_id.clone(),
                platform: scoring::ROUND_PLATFORM.to_string(),
                observed_name: observed_name.clone(),
                observed_name_bytes_hex: observed_name
                    .as_ref()
                    .map(|s| s.as_bytes().iter().map(|b| format!("{b:02x}")).collect()),
                observed_role: observed_role.clone(),
                prohibited_outcome: prohibited_outcome.clone(),
                pass: false,
                notes: reason.clone(),
            },
            FixtureCheck5Outcome::NotRun { reason } => A11yEvidence {
                fixture_id: fixture_id.clone(),
                platform: scoring::ROUND_PLATFORM.to_string(),
                observed_name: None,
                observed_name_bytes_hex: None,
                observed_role: None,
                prohibited_outcome: None,
                pass: false,
                notes: reason.clone(),
            },
        })
        .collect();

    let (check5_accessibility, check5_bus_unreachable_evidence) = aggregate_check5(&outcomes);

    Ok(Check5Result {
        check5_accessibility,
        check5_bus_unreachable_evidence,
        a11y_evidence,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "c1-a11y-subprocess-test-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_json(path: &Path, fixture_id: &str, verdict: &str) {
        std::fs::write(
            path,
            serde_json::json!({
                "fixture_id": fixture_id,
                "verdict": verdict,
                "reason": "test",
                "observed_role": "label",
                "observed_name": "x",
                "observed_name_hex": "78",
                "prohibited_outcome": serde_json::Value::Null,
                "walked_tree": [],
            })
            .to_string(),
        )
        .unwrap();
    }

    // ---- F1: freshness ----

    /// Required kill: a stale PASS file sitting at the path this
    /// invocation was told to use, when the exit status is 2 (usage error
    /// / NOT RUN, which per verify.py's own contract never writes --json),
    /// must be refused — never silently read as this run's own PASS.
    #[test]
    fn a_stale_pass_file_at_exit_2_is_refused_not_picked_up() {
        let dir = scratch_dir("stale-pass-exit-2");
        let path = dir.join("F-A-stale.json");
        write_json(&path, "F-A", "PASS");
        let err = interpret_verify_output(&path, Some(2), "F-A", "CHECK5: NOT RUN — bus gone")
            .unwrap_err();
        assert!(
            err.to_string().contains("exit 2"),
            "must name the exit-2/file-exists contradiction: {err}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The legitimate case this must not break: exit 2, no file present,
    /// verify.py's own "NOT RUN" prefix (bus genuinely unreachable) ->
    /// admissible NotRun.
    #[test]
    fn a_genuine_bus_unreachable_exit_2_with_no_file_is_not_run() {
        let dir = scratch_dir("genuine-not-run");
        let path = dir.join("F-A-fresh.json");
        let outcome = interpret_verify_output(
            &path,
            Some(2),
            "F-A",
            "CHECK5: NOT RUN — Atspi.init() failed",
        )
        .unwrap();
        assert!(matches!(outcome, FixtureCheck5Outcome::NotRun { .. }));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Required kill: exit 2 with no fresh file present but verify.py's own
    /// output says "usage error" (a defect in how THIS candidate invoked
    /// it, e.g. a bad digest or path) must be a hard failure, never
    /// admissible bus-unreachable evidence — the exact "every exit 2
    /// becomes admissible" failure mode F1 names.
    #[test]
    fn a_usage_error_at_exit_2_is_a_hard_failure_not_admissible_not_run() {
        let dir = scratch_dir("usage-error");
        let path = dir.join("F-A-fresh.json");
        let err = interpret_verify_output(
            &path,
            Some(2),
            "F-A",
            "CHECK5: usage error — 'a11y_expectations.json' failed validation: ...",
        )
        .unwrap_err();
        assert!(err.to_string().contains("usage error"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// G1 required kill: empty stdout at exit 2 must be a hard failure, not
    /// admissible NOT RUN — the exact "almost anything qualifies" bug: no
    /// marker at all was previously sufficient because it merely didn't
    /// start with "CHECK5: usage error".
    #[test]
    fn g1_empty_stdout_at_exit_2_is_a_hard_failure() {
        let dir = scratch_dir("g1-empty-stdout");
        let path = dir.join("F-A-fresh.json");
        let err = interpret_verify_output(&path, Some(2), "F-A", "").unwrap_err();
        assert!(
            err.to_string()
                .contains("without an approved NOT-RUN marker"),
            "{err}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// G1 required kill: unrecognised stdout (present, non-empty, but
    /// naming nothing verify.py's own NOT-RUN exit points actually print)
    /// must be a hard failure.
    #[test]
    fn g1_unrecognised_stdout_at_exit_2_is_a_hard_failure() {
        let dir = scratch_dir("g1-unrecognised-stdout");
        let path = dir.join("F-A-fresh.json");
        let err = interpret_verify_output(
            &path,
            Some(2),
            "F-A",
            "some unrelated diagnostic banner, not one of verify.py's own messages",
        )
        .unwrap_err();
        assert!(
            err.to_string()
                .contains("without an approved NOT-RUN marker"),
            "{err}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// G1 required kill: an argparse-style failure (Python's argparse
    /// writes usage errors to **stderr**, leaving stdout empty) must be a
    /// hard failure — this function only ever sees stdout, so this is the
    /// same code path as the empty-stdout case, exercised under its own
    /// name because it is the specific real-world trigger G1 names.
    #[test]
    fn g1_argparse_stderr_only_failure_is_a_hard_failure() {
        let dir = scratch_dir("g1-argparse-stderr-only");
        let path = dir.join("F-A-fresh.json");
        // stdout empty, as it would be for a real argparse ap.error() exit
        // (argparse prints usage + the error to stderr and calls
        // sys.exit(2), never touching stdout).
        let err = interpret_verify_output(&path, Some(2), "F-A", "").unwrap_err();
        assert!(
            err.to_string()
                .contains("without an approved NOT-RUN marker"),
            "{err}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// G1: a genuine "CHECK5: NOT RUN" with one of every approved marker
    /// must still be admitted — the allow-list must not have become so
    /// strict it refuses verify.py's own real exit points.
    #[test]
    fn g1_every_approved_marker_is_admitted() {
        let dir = scratch_dir("g1-every-marker");
        for marker in APPROVED_NOT_RUN_MARKERS {
            let stdout = format!("CHECK5: NOT RUN — {marker}: simulated");
            let path = dir.join("F-A-fresh.json");
            let outcome = interpret_verify_output(&path, Some(2), "F-A", &stdout)
                .unwrap_or_else(|e| panic!("marker {marker:?} must be admitted: {e}"));
            assert!(matches!(outcome, FixtureCheck5Outcome::NotRun { .. }));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// G1 gap fix, half one: the `"CHECK5: NOT RUN"` prefix is present, but
    /// nothing after it matches any [`APPROVED_NOT_RUN_MARKERS`] entry.
    /// verify.py itself never actually emits this (every real NOT-RUN exit
    /// point pairs the prefix with one of the approved markers), but the
    /// guard against it is what actually proves the **marker** half of the
    /// conjunction is load-bearing: this is the one case that distinguishes
    /// `prefix && marker` from `prefix && true` (a marker check silently
    /// dropped) — every case already covered (empty stdout, unrelated
    /// stdout, argparse's stderr-only failure) lacks the prefix too, so a
    /// dropped marker check could not be seen through those alone.
    #[test]
    fn g1_prefix_present_marker_absent_is_a_hard_failure() {
        let dir = scratch_dir("g1-prefix-no-marker");
        let path = dir.join("F-A-fresh.json");
        let err = interpret_verify_output(
            &path,
            Some(2),
            "F-A",
            "CHECK5: NOT RUN — something we do not recognise",
        )
        .unwrap_err();
        assert!(
            err.to_string()
                .contains("without an approved NOT-RUN marker"),
            "{err}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// G1 gap fix, half two: a marker substring appears somewhere in the
    /// output, but the line does not carry the `"CHECK5: NOT RUN"` prefix —
    /// an unrelated diagnostic that happens to mention e.g. `Atspi.init()
    /// failed` is not a NOT-RUN verdict. This is the case that distinguishes
    /// `prefix && marker` from `true && marker` (a prefix check silently
    /// dropped) — every case already covered lacks a marker too, so a
    /// dropped prefix check could not be seen through those alone.
    #[test]
    fn g1_marker_present_prefix_absent_is_a_hard_failure() {
        let dir = scratch_dir("g1-marker-no-prefix");
        let path = dir.join("F-A-fresh.json");
        let err = interpret_verify_output(
            &path,
            Some(2),
            "F-A",
            "unrelated preamble mentioning Atspi.init() failed in passing, not a verdict line",
        )
        .unwrap_err();
        assert!(
            err.to_string()
                .contains("without an approved NOT-RUN marker"),
            "{err}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Required kill: exit 0 (PASS) but no file at the fresh path at all —
    /// must error rather than silently treat the run as failed/not-run.
    #[test]
    fn exit_0_with_no_file_is_a_hard_error() {
        let dir = scratch_dir("exit0-no-file");
        let path = dir.join("F-A-missing.json");
        let err = interpret_verify_output(&path, Some(0), "F-A", "").unwrap_err();
        assert!(err.to_string().contains("wrote no JSON"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Required kill: exit status and JSON verdict disagreement (exit 0
    /// implies PASS, but the JSON says FAIL) must be a hard error, never
    /// resolved by trusting either source silently.
    #[test]
    fn exit_status_and_json_verdict_disagreement_is_a_hard_error() {
        let dir = scratch_dir("disagreement");
        let path = dir.join("F-A.json");
        write_json(&path, "F-A", "FAIL");
        let err = interpret_verify_output(&path, Some(0), "F-A", "").unwrap_err();
        assert!(err.to_string().contains("disagree"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Required kill: the JSON's own `fixture_id` must match what was
    /// requested — otherwise a result meant for a different fixture (e.g. a
    /// path mixup) could be silently attributed to this one.
    #[test]
    fn a_mismatched_fixture_id_in_the_json_is_refused() {
        let dir = scratch_dir("mismatched-fixture");
        let path = dir.join("F-A.json");
        write_json(&path, "F-B", "PASS"); // wrong fixture id inside the file
        let err = interpret_verify_output(&path, Some(0), "F-A", "").unwrap_err();
        assert!(err.to_string().contains("fixture_id"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The legitimate PASS path still works after all the above refusals.
    #[test]
    fn a_genuine_matching_pass_is_accepted() {
        let dir = scratch_dir("genuine-pass");
        let path = dir.join("F-A.json");
        write_json(&path, "F-A", "PASS");
        let outcome = interpret_verify_output(&path, Some(0), "F-A", "").unwrap();
        assert!(matches!(outcome, FixtureCheck5Outcome::Pass { .. }));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// [`fresh_temp_path`] never repeats across calls, even for the same
    /// fixture id in the same process — the structural property F1's whole
    /// fix rests on.
    #[test]
    fn fresh_temp_path_never_repeats() {
        let mut seen = std::collections::HashSet::new();
        for _ in 0..50 {
            let p = fresh_temp_path("F-A");
            assert!(seen.insert(p.clone()), "path repeated: {}", p.display());
            assert!(
                !p.exists(),
                "a fresh path must never already exist: {}",
                p.display()
            );
        }
    }

    // ---- F2: ordering independence ----

    fn fail(id: &str) -> (String, FixtureCheck5Outcome) {
        (
            id.to_string(),
            FixtureCheck5Outcome::Fail {
                reason: "absent-from-tree".to_string(),
                observed_role: None,
                observed_name: None,
                prohibited_outcome: Some("absent-from-tree".to_string()),
            },
        )
    }
    fn not_run(id: &str) -> (String, FixtureCheck5Outcome) {
        (
            id.to_string(),
            FixtureCheck5Outcome::NotRun {
                reason: "bus unreachable".to_string(),
            },
        )
    }
    fn pass(id: &str) -> (String, FixtureCheck5Outcome) {
        (
            id.to_string(),
            FixtureCheck5Outcome::Pass {
                observed_role: Some("label".to_string()),
                observed_name: Some("x".to_string()),
            },
        )
    }

    /// Required kill: FAIL-then-NotRun must still report FAIL, not NotRun —
    /// the exact bug (a known FAIL erased by a later bus-unreachable).
    #[test]
    fn fail_then_not_run_reports_fail() {
        let outcomes = vec![fail("F-A"), not_run("F-B")];
        let (outcome, evidence) = aggregate_check5(&outcomes);
        assert!(outcome.is_fail(), "{outcome:?}");
        assert!(evidence.is_none());
    }

    /// The other ordering must produce the SAME outcome — proving the
    /// aggregation does not depend on which was observed first.
    #[test]
    fn not_run_then_fail_reports_fail_too() {
        let outcomes = vec![not_run("F-A"), fail("F-B")];
        let (outcome, evidence) = aggregate_check5(&outcomes);
        assert!(outcome.is_fail(), "{outcome:?}");
        assert!(evidence.is_none());
    }

    #[test]
    fn all_pass_reports_pass() {
        let outcomes = vec![pass("F-A"), pass("F-B")];
        let (outcome, evidence) = aggregate_check5(&outcomes);
        assert!(outcome.is_pass());
        assert!(evidence.is_none());
    }

    /// NotRun with no FAIL anywhere is admissible, and carries evidence.
    #[test]
    fn not_run_with_no_fail_is_admissible_with_evidence() {
        let outcomes = vec![pass("F-A"), not_run("F-B")];
        let (outcome, evidence) = aggregate_check5(&outcomes);
        assert!(outcome.is_not_run(), "{outcome:?}");
        assert!(evidence.is_some());
    }

    #[test]
    fn not_run_before_pass_is_admissible_too() {
        let outcomes = vec![not_run("F-A"), pass("F-B")];
        let (outcome, evidence) = aggregate_check5(&outcomes);
        assert!(outcome.is_not_run(), "{outcome:?}");
        assert!(evidence.is_some());
    }

    // ---- G3: publish, don't accumulate ----

    /// Required kill: publishing five fixtures' outcomes, twice in a row,
    /// must leave **exactly five** canonical files — never ten. This is the
    /// exact defect G3 closes: the earlier revision wrote a fresh
    /// PID/timestamp-named file straight into `evidence_dir` on every run,
    /// so `evidence_dir` accumulated without bound.
    #[test]
    fn publishing_is_idempotent_two_runs_leave_exactly_five_files() {
        let dir = scratch_dir("idempotent-publish");
        let fixture_ids = ["F-A", "F-B", "F-C", "F-D", "F-E"];
        for round in 0..2 {
            for id in fixture_ids {
                let temp = dir.join(format!("scratch-{id}-{round}.json"));
                write_json(&temp, id, "PASS");
                let outcome = FixtureCheck5Outcome::Pass {
                    observed_role: Some("label".to_string()),
                    observed_name: Some("x".to_string()),
                };
                publish_canonical(&dir, id, &temp, &outcome).unwrap();
                let _ = std::fs::remove_file(&temp);
            }
            let files: Vec<_> = std::fs::read_dir(&dir)
                .unwrap()
                .map(|e| e.unwrap().file_name())
                .collect();
            assert_eq!(
                files.len(),
                5,
                "round {round}: expected exactly 5 canonical files, got {files:?}"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A `Pass`/`Fail` outcome's canonical file carries the actually-
    /// published content (not merely "a file exists") — republishing with a
    /// different verdict must overwrite, not append.
    #[test]
    fn publishing_overwrites_the_previous_verdict() {
        let dir = scratch_dir("overwrite-verdict");
        let temp1 = dir.join("scratch-1.json");
        write_json(&temp1, "F-A", "PASS");
        publish_canonical(
            &dir,
            "F-A",
            &temp1,
            &FixtureCheck5Outcome::Pass {
                observed_role: None,
                observed_name: None,
            },
        )
        .unwrap();

        let temp2 = dir.join("scratch-2.json");
        write_json(&temp2, "F-A", "FAIL");
        publish_canonical(
            &dir,
            "F-A",
            &temp2,
            &FixtureCheck5Outcome::Fail {
                reason: "x".to_string(),
                observed_role: None,
                observed_name: None,
                prohibited_outcome: None,
            },
        )
        .unwrap();

        let canonical = dir.join("F-A.json");
        let text = std::fs::read_to_string(&canonical).unwrap();
        assert!(text.contains("FAIL"), "{text}");
        assert!(!text.contains("PASS"), "{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A `NotRun` outcome removes any stale canonical file rather than
    /// leaving an earlier run's PASS/FAIL lingering under a claim this run
    /// never made.
    #[test]
    fn a_not_run_outcome_removes_a_stale_canonical_file() {
        let dir = scratch_dir("not-run-removes-stale");
        let temp1 = dir.join("scratch-1.json");
        write_json(&temp1, "F-A", "PASS");
        publish_canonical(
            &dir,
            "F-A",
            &temp1,
            &FixtureCheck5Outcome::Pass {
                observed_role: None,
                observed_name: None,
            },
        )
        .unwrap();
        assert!(dir.join("F-A.json").exists());

        // No temp file exists for a NotRun outcome (verify.py never wrote
        // one) — pass a path that does not exist, matching the real
        // caller's situation.
        let missing_temp = dir.join("does-not-exist.json");
        publish_canonical(
            &dir,
            "F-A",
            &missing_temp,
            &FixtureCheck5Outcome::NotRun {
                reason: "bus unreachable".to_string(),
            },
        )
        .unwrap();
        assert!(
            !dir.join("F-A.json").exists(),
            "a stale canonical file must be removed on NotRun, not left claiming PASS"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
