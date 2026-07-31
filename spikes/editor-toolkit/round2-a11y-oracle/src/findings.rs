//! Findings routed back to `spikes/editor-toolkit/ROUND2_TEXT_RECIPE.md`,
//! discovered while building this crate.
//!
//! Recorded here — not only in a review conversation — so whoever next
//! revises the recipe finds it in the artifact rather than a chat transcript,
//! the same discipline `round2_textkit::findings` uses for the findings it
//! routes back to the W3 `.tex` amendment. These are findings *about the
//! recipe's own prose*, not about `epiphany-layout-ir`, so they are recorded
//! here rather than in `round2_textkit::findings`.
//!
//! This crate does not edit `ROUND2_TEXT_RECIPE.md` — that document belongs
//! to the coordinator's commit and a separate review thread.

/// Recipe §8.1 claims: "a tree assembled by walking the visual runs left to
/// right produces a different string, and only there \[F-D\]."
///
/// That is false under this crate's own generated data. F-B diverges the
/// same way: its logical name is `"Coro אבג"` and
/// `round2_a11y_oracle::visual_order_form` produces `"Coro גבא"` for it — a
/// real, non-empty `visual_order_name` entry in `a11y_expectations.json`,
/// exactly the same mechanism F-D exercises.
///
/// The general shape, not just the one counterexample: under
/// [`crate::visual_order_form`]'s model (concatenate segments in stored
/// order, reversing an `Rtl` segment's own text by grapheme), **any
/// non-palindromic** RTL run of two or more graphemes diverges under a
/// visual-order walk, because reversing a grapheme sequence is a no-op
/// exactly when that sequence is a palindrome (a repeated single grapheme,
/// e.g. `"aa"`, is a palindrome and is therefore **not** a counterexample to
/// this narrower claim — it was a counterexample to the unqualified "any RTL
/// run of two or more graphemes" claim an earlier revision of this finding
/// made). F-D is not the *unique* case; it is the case where the RTL run is
/// *interior* to the string (`"Allegro "` ... `"אבג"` ... `" con brio"`)
/// rather than trailing (`"Coro "` ... `"אבג"`), which is why F-D's
/// divergence reads as obviously wrong to a human glancing at it and F-B's —
/// a suffix silently reversed — reads as more easily missed. That
/// readability difference is a real reason to prefer F-D as the check-5
/// accessibility exemplar; it is not a reason to claim F-B does not exhibit
/// the same property.
///
/// The recipe should either say "F-D and F-B" at §8.1, or drop the
/// uniqueness claim and state the actual distinguishing property: F-D is the
/// fixture where the RTL run is interior, not the fixture where the
/// divergence uniquely occurs.
///
/// ## The same stale claim is also baked into a digest-bound artifact
///
/// The recipe's prose is not the only place this claim lives.
/// `round2-textkit/src/a11y.rs`'s `note_for("F-D")` reads: "the concatenation
/// is logical-order, so a tree built by walking the visual runs left to
/// right fails here and only here" — the identical uniqueness claim, in
/// code. That note is compiled into every generated `fixtures.json` as
/// `fixtures[3].accessibility.note`, and `fixtures.json`'s own
/// `EXPECTED_ARTIFACT_DIGEST_HEX` (`round2_textkit::output`) binds the whole
/// serialized file, note text included, to `acc13c0d…` — a frozen,
/// user-reviewed artifact (Packet 2A). Editing the note's wording to correct
/// the claim would change that digest and break every consumer pinned to it,
/// which is a strictly larger and differently-scoped change than this
/// finding.
///
/// **This half of the finding is tracked, not fixed**, and is recorded
/// explicitly so a later reader does not "helpfully" edit
/// `round2-textkit/src/a11y.rs`'s F-D note on the strength of this finding
/// alone and silently move `acc13c0d…` out from under Packet 2A. Fixing it
/// is a decision for whoever owns that digest and that packet's re-freeze,
/// not a drive-by edit from this crate.
pub const RECIPE_F1_VISUAL_ORDER_NOT_UNIQUE_TO_F_D: &str = "ROUND2_TEXT_RECIPE.md §8.1 claims \
    visual-order-walk assembly produces a different string \"and only there [F-D]\". It does \
    not: F-B's visual_order_name (\"Coro גבא\") also differs from its expected_name (\"Coro \
    אבג\"), and under this crate's visual_order_form, any non-palindromic RTL run of two or more \
    graphemes diverges the same way (a repeated-grapheme run like \"aa\" is a palindrome and does \
    not diverge, which is why the claim is qualified). F-D is not unique in exhibiting the \
    divergence; it is the fixture where the RTL run is interior to the string rather than \
    trailing, which is why the divergence is more obviously wrong to a reader. The recipe should \
    say \"F-D and F-B\" or state the interior-run property instead of a uniqueness claim. The \
    identical stale claim is also baked into round2-textkit/src/a11y.rs's note_for(\"F-D\") \
    (\"fails here and only here\"), which is compiled into fixtures.json and covered by its \
    frozen EXPECTED_ARTIFACT_DIGEST_HEX (acc13c0d...) — that half is TRACKED, NOT FIXED here, \
    because correcting it would move the digest and break Packet 2A; do not edit that note on \
    the strength of this finding alone.";

#[cfg(test)]
mod tests {
    use super::*;

    /// The finding must actually name both fixtures — a mutation that
    /// silently dropped one of them from the constant would still compile
    /// and would still "record a finding," just not the right one.
    #[test]
    fn the_finding_names_both_f_d_and_f_b() {
        assert!(RECIPE_F1_VISUAL_ORDER_NOT_UNIQUE_TO_F_D.contains("F-D"));
        assert!(RECIPE_F1_VISUAL_ORDER_NOT_UNIQUE_TO_F_D.contains("F-B"));
    }

    /// The universal claim must be qualified — an unqualified "any RTL run
    /// of two or more graphemes diverges" is false (a palindromic run does
    /// not), which is exactly the over-claim B3 asked to be narrowed.
    #[test]
    fn the_finding_qualifies_the_claim_as_non_palindromic() {
        assert!(RECIPE_F1_VISUAL_ORDER_NOT_UNIQUE_TO_F_D.contains("non-palindromic"));
    }

    /// The digest-bound, tracked-not-fixed half of the finding must name the
    /// actual frozen digest prefix and say explicitly that it is not fixed
    /// here — a reader skimming only for "is this fixed" must not be able to
    /// mistake "recorded" for "corrected."
    #[test]
    fn the_finding_names_the_frozen_digest_and_says_tracked_not_fixed() {
        assert!(RECIPE_F1_VISUAL_ORDER_NOT_UNIQUE_TO_F_D.contains("acc13c0d"));
        assert!(RECIPE_F1_VISUAL_ORDER_NOT_UNIQUE_TO_F_D.contains("TRACKED, NOT FIXED"));
        assert!(RECIPE_F1_VISUAL_ORDER_NOT_UNIQUE_TO_F_D.contains("a11y.rs"));
    }
}
