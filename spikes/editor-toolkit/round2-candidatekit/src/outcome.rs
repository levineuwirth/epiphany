//! The per-check outcome type both candidates report against.

use serde::{Deserialize, Deserializer, Serialize};

/// One check's outcome. Exactly three states, and **both non-`Pass` states
/// carry a reason**: pin 14 requires an environmental `NotRun` to record
/// *why* it could not run, and a bare `Fail` with no reason would be
/// exactly the unfalsifiable report `round1-oracle`'s discipline exists to
/// forbid. There is deliberately no unit-only `NotRun` or `Fail` variant —
/// a candidate cannot report "did not pass" without saying why.
///
/// **An empty or whitespace-only reason is a bare reason wearing a
/// string.** The checked constructors ([`CheckOutcome::fail`],
/// [`CheckOutcome::not_run`]) and this type's `Deserialize` impl both
/// reject one — those are the two paths a candidate actually uses to
/// produce a `CandidateReport` (build it in Rust, or read one back from
/// JSON). The variants' payloads stay `pub` because a fully private field
/// would need a getter/setter pair that adds ceremony without closing any
/// path a candidate is expected to take; the invalid state is
/// unconstructible through construction *and* deserialization, which is
/// what "a reason is required" needs to mean in practice.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum CheckOutcome {
    Pass,
    Fail(String),
    NotRun(String),
}

impl CheckOutcome {
    /// Checked constructor: rejects an empty-or-whitespace-only reason.
    pub fn fail(reason: impl Into<String>) -> Result<Self, String> {
        let reason = reason.into();
        if reason.trim().is_empty() {
            return Err(
                "CheckOutcome::fail: reason must not be empty or whitespace-only — a Fail with \
                 no reason is exactly the unfalsifiable report this type exists to forbid"
                    .to_string(),
            );
        }
        Ok(CheckOutcome::Fail(reason))
    }

    /// Checked constructor: rejects an empty-or-whitespace-only reason.
    pub fn not_run(reason: impl Into<String>) -> Result<Self, String> {
        let reason = reason.into();
        if reason.trim().is_empty() {
            return Err(
                "CheckOutcome::not_run: reason must not be empty or whitespace-only — pin 14 \
                 requires the environmental cause to be recorded, not merely gestured at"
                    .to_string(),
            );
        }
        Ok(CheckOutcome::NotRun(reason))
    }

    /// Ordering used by [`crate::scoring::criterion_cell`]'s worst-of-five
    /// rule: `Pass` < `NotRun` < `Fail`. Higher is worse.
    pub(crate) fn severity_rank(&self) -> u8 {
        match self {
            CheckOutcome::Pass => 0,
            CheckOutcome::NotRun(_) => 1,
            CheckOutcome::Fail(_) => 2,
        }
    }

    pub fn is_pass(&self) -> bool {
        matches!(self, CheckOutcome::Pass)
    }

    pub fn is_fail(&self) -> bool {
        matches!(self, CheckOutcome::Fail(_))
    }

    pub fn is_not_run(&self) -> bool {
        matches!(self, CheckOutcome::NotRun(_))
    }
}

/// The wire shape `CheckOutcome` deserializes through — identical variants
/// and payloads, `#[serde(deny_unknown_fields)]` for the same structural-
/// drift reason every deserializable type in this workspace uses it, kept
/// as a **separate, private** type so [`CheckOutcome`]'s own `Deserialize`
/// impl can run [`CheckOutcome::fail`]/[`CheckOutcome::not_run`]'s
/// empty-reason check on the way through, which `#[derive(Deserialize)]`
/// on `CheckOutcome` directly could not do.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
enum CheckOutcomeWire {
    Pass,
    Fail(String),
    NotRun(String),
}

impl TryFrom<CheckOutcomeWire> for CheckOutcome {
    type Error = String;

    fn try_from(wire: CheckOutcomeWire) -> Result<Self, String> {
        match wire {
            CheckOutcomeWire::Pass => Ok(CheckOutcome::Pass),
            CheckOutcomeWire::Fail(reason) => CheckOutcome::fail(reason),
            CheckOutcomeWire::NotRun(reason) => CheckOutcome::not_run(reason),
        }
    }
}

impl<'de> Deserialize<'de> for CheckOutcome {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = CheckOutcomeWire::deserialize(deserializer)?;
        CheckOutcome::try_from(wire).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_orders_pass_below_not_run_below_fail() {
        assert!(
            CheckOutcome::Pass.severity_rank() < CheckOutcome::NotRun("x".into()).severity_rank()
        );
        assert!(
            CheckOutcome::NotRun("x".into()).severity_rank()
                < CheckOutcome::Fail("x".into()).severity_rank()
        );
    }

    #[test]
    fn predicates_agree_with_the_variant() {
        assert!(CheckOutcome::Pass.is_pass());
        assert!(!CheckOutcome::Pass.is_fail());
        assert!(!CheckOutcome::Pass.is_not_run());

        assert!(CheckOutcome::Fail("x".into()).is_fail());
        assert!(!CheckOutcome::Fail("x".into()).is_pass());

        assert!(CheckOutcome::NotRun("x".into()).is_not_run());
        assert!(!CheckOutcome::NotRun("x".into()).is_pass());
    }

    #[test]
    fn round_trips_through_json() {
        for outcome in [
            CheckOutcome::Pass,
            CheckOutcome::Fail("reason".to_string()),
            CheckOutcome::NotRun("reason".to_string()),
        ] {
            let json = serde_json::to_string(&outcome).unwrap();
            let back: CheckOutcome = serde_json::from_str(&json).unwrap();
            assert_eq!(outcome, back);
        }
    }

    // ---- F6: a real distinguishing assertion, not `len() > 0` ----

    /// A bare JSON string `"NotRun"` does not match the tuple-variant shape
    /// `NotRun(String)` at all (that shape serializes as
    /// `{"NotRun": "..."}`), so this is a **structural** deserialize
    /// failure — distinct from the empty-reason rejection below, which
    /// targets a `NotRun` that *does* carry a payload, just an empty one.
    /// Asserts on serde's actual reported type mismatch (a unit-shaped
    /// value where a payload-carrying variant was required), which is what
    /// actually distinguishes this rejection from every other kind of
    /// deserialize failure this file tests — not on "some error happened"
    /// (measured: `err.to_string()` is `"invalid type: unit variant,
    /// expected newtype variant"`, which names neither `NotRun` nor `Fail`
    /// by name, so asserting on the variant name would itself have been
    /// wrong).
    #[test]
    fn a_bare_string_not_run_with_no_payload_fails_to_deserialize() {
        let bad = serde_json::json!("NotRun");
        let err = serde_json::from_value::<CheckOutcome>(bad)
            .unwrap_err()
            .to_string();
        assert!(err.contains("unit variant"), "{err}");
        assert!(err.contains("newtype variant"), "{err}");
    }

    // ---- F3: an empty or whitespace-only reason is refused ----

    #[test]
    fn the_fail_constructor_rejects_an_empty_reason() {
        let err = CheckOutcome::fail("").unwrap_err();
        assert!(err.contains("empty"), "{err}");
    }

    #[test]
    fn the_fail_constructor_rejects_a_whitespace_only_reason() {
        let err = CheckOutcome::fail("   \t  ").unwrap_err();
        assert!(err.contains("empty"), "{err}");
    }

    #[test]
    fn the_fail_constructor_accepts_a_real_reason() {
        let outcome = CheckOutcome::fail("host-substituted the Hebrew segment").unwrap();
        assert_eq!(
            outcome,
            CheckOutcome::Fail("host-substituted the Hebrew segment".to_string())
        );
    }

    #[test]
    fn the_not_run_constructor_rejects_an_empty_reason() {
        let err = CheckOutcome::not_run("").unwrap_err();
        assert!(err.contains("empty"), "{err}");
    }

    #[test]
    fn the_not_run_constructor_rejects_a_whitespace_only_reason() {
        let err = CheckOutcome::not_run("\n").unwrap_err();
        assert!(err.contains("empty"), "{err}");
    }

    #[test]
    fn the_not_run_constructor_accepts_a_real_reason() {
        let outcome = CheckOutcome::not_run("no Arabic-capable face installed").unwrap();
        assert_eq!(
            outcome,
            CheckOutcome::NotRun("no Arabic-capable face installed".to_string())
        );
    }

    /// Guards the deserialize path the same way the constructors guard
    /// direct construction: a `Fail` with an empty string payload must be
    /// refused on the way in from JSON, not merely by a constructor a
    /// candidate could route around by deserializing instead.
    #[test]
    fn deserializing_an_empty_reason_fail_is_refused() {
        let bad = serde_json::json!({"Fail": ""});
        let err = serde_json::from_value::<CheckOutcome>(bad)
            .unwrap_err()
            .to_string();
        assert!(err.contains("empty"), "{err}");
    }

    #[test]
    fn deserializing_a_whitespace_only_reason_not_run_is_refused() {
        let bad = serde_json::json!({"NotRun": "   "});
        let err = serde_json::from_value::<CheckOutcome>(bad)
            .unwrap_err()
            .to_string();
        assert!(err.contains("empty"), "{err}");
    }

    #[test]
    fn deserializing_a_real_reason_still_works() {
        let good = serde_json::json!({"Fail": "a real reason"});
        let outcome: CheckOutcome = serde_json::from_value(good).unwrap();
        assert_eq!(outcome, CheckOutcome::Fail("a real reason".to_string()));
    }

    /// An unknown variant name must still be refused — `CheckOutcomeWire`'s
    /// own shape carries forward through the custom `Deserialize` impl
    /// rather than being silently lost when `CheckOutcome` stopped deriving
    /// it directly. Measured: `err.to_string()` is `"unknown variant
    /// \`Passed\`, expected one of \`Pass\`, \`Fail\`, \`NotRun\`"`, so the
    /// specific bad name is named in the message.
    #[test]
    fn an_unknown_variant_name_is_refused() {
        let bad = serde_json::json!({"Passed": null});
        let err = serde_json::from_value::<CheckOutcome>(bad)
            .unwrap_err()
            .to_string();
        assert!(err.contains("unknown variant"), "{err}");
        assert!(err.contains("Passed"), "{err}");
    }
}
