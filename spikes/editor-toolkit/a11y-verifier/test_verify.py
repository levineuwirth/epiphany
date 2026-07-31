"""Plain `unittest` coverage for `verify.classify` and
`verify.validate_expectations_file` — check 5's whole scoring logic and its
fail-closed artifact validation, isolated from any live AT-SPI bus.

`verify.py`'s `gi.repository.Atspi` import happens inside `main()`, not at
module import time, specifically so this file can `import verify` and drive
`classify()` directly with synthetic `ObservedNode` trees — no bus, no
desktop, no display required. Run with:

    python3 -m unittest

from this directory (`a11y-verifier/`).

Every test constructs the *wrong* input for the property it names and
asserts the *specific* verdict/outcome that input must produce — never a
bare "FAIL" or "not PASS", because a classifier branch that silently
degrades to the wrong FAIL reason would pass a weaker test and is exactly
the kind of regression this file exists to catch.
"""
import unittest

from verify import (
    EXPECTED_FIXTURE_IDS,
    PLATFORM,
    VISUAL_ORDER_TRAP,
    ObservedNode,
    Verdict,
    _is_source_bearing_fragment,
    classify,
    validate_expectations_file,
)


def node(role, name, children=None):
    """A synthetic `ObservedNode`, the same shape `walk_for_check5` builds
    from a live AT-SPI tree."""
    return ObservedNode(role=role, name=name, children=list(children) if children else [])


def expectation(
    expected_name,
    accepted_roles=("label", "static", "text", "paragraph"),
    prohibited_roles=("image", "canvas", "filler", "panel", "unknown"),
    alternative_forms=None,
    visual_order_name=None,
    source_atoms=None,
):
    """A synthetic fixture entry with the same shape
    `round2-a11y-oracle/a11y_expectations.json` emits."""
    return {
        "fixture_id": "F-TEST",
        "expected_name": expected_name,
        "expected_name_hex": expected_name.encode("utf-8").hex(),
        "expected_name_byte_len": len(expected_name.encode("utf-8")),
        "accepted_roles": list(accepted_roles),
        "prohibited_roles": list(prohibited_roles),
        "alternative_forms": alternative_forms or {},
        "visual_order_name": visual_order_name,
        # D1: per-segment source atoms (`round2-a11y-oracle`'s
        # `source_atoms`). Defaults to `None` (classify's own `.get(...) or
        # []` treats that as no atoms), since most tests don't need one.
        "source_atoms": source_atoms,
    }


class ByteExactPass(unittest.TestCase):
    def test_single_node_with_accepted_role_and_exact_name_passes(self):
        exp = expectation("Allegro affettuoso — al fine")
        verdict = classify(exp, [node("text", "Allegro affettuoso — al fine")])
        self.assertEqual(verdict.verdict, "PASS")
        self.assertIsNone(verdict.prohibited_outcome)
        self.assertEqual(verdict.observed_role, "text")
        self.assertEqual(verdict.observed_name, exp["expected_name"])

    def test_a_one_byte_difference_does_not_pass(self):
        """Mutation guard: if byte comparison were replaced by e.g. a
        case-insensitive or trimmed comparison, this would wrongly PASS."""
        exp = expectation("Allegro")
        verdict = classify(exp, [node("text", "allegro")])
        self.assertNotEqual(verdict.verdict, "PASS")


class CompositionConcatenationPass(unittest.TestCase):
    def test_two_segment_names_concatenated_in_tree_order_pass(self):
        exp = expectation("Coro אבג")
        # No single node carries the whole name — only the concatenation of
        # two text descendants of a common parent, in tree/logical order,
        # does.
        run = node("frame", "", children=[node("text", "Coro "), node("text", "אבג")])
        verdict = classify(exp, [run])
        self.assertEqual(verdict.verdict, "PASS")
        self.assertIsNone(verdict.prohibited_outcome)
        self.assertEqual(verdict.observed_name, exp["expected_name"])

    def test_concatenation_in_the_wrong_order_does_not_pass(self):
        """Mutation guard: if concatenation order were unspecified (e.g. a
        set instead of an ordered list), swapping the two nodes would still
        wrongly PASS."""
        exp = expectation("Coro אבג")
        run = node("frame", "", children=[node("text", "אבג"), node("text", "Coro ")])
        verdict = classify(exp, [run])
        self.assertNotEqual(verdict.verdict, "PASS")


class ProhibitedOutcomes(unittest.TestCase):
    """One test per `round2_textkit::a11y::PROHIBITED_OUTCOMES` name."""

    def test_absent_from_tree_when_no_candidate_node_is_found(self):
        exp = expectation("Allegro")
        verdict = classify(exp, [])
        self.assertEqual(verdict.verdict, "FAIL")
        self.assertEqual(verdict.prohibited_outcome, "absent-from-tree")

    def test_name_empty_is_distinct_from_absent_from_tree(self):
        """§8.3: "name-empty ... absence wearing a role." A node is present
        (unlike the absent-from-tree case above) but its name is the empty
        string — these must classify differently."""
        exp = expectation("Allegro")
        verdict = classify(exp, [node("label", "")])
        self.assertEqual(verdict.verdict, "FAIL")
        self.assertEqual(verdict.prohibited_outcome, "name-empty")

    def test_name_normalized_matches_the_precommitted_nfc_form(self):
        # F-E's case: source is NFD ("Cafe" + combining acute, "Café"),
        # a wrong tree exposes the NFC "Café" ("Café", a *different*
        # string byte-for-byte even though the two render identically)
        # instead. Written with explicit \N escapes rather than the literal
        # glyph so the two forms cannot be silently typed as the same string
        # by accident — that mistake produced a false PASS here once already.
        nfd = "Cafe\N{COMBINING ACUTE ACCENT}"
        nfc = "Caf\N{LATIN SMALL LETTER E WITH ACUTE}"
        self.assertNotEqual(nfd, nfc, "anchor: the two forms must be different strings")
        exp = expectation(
            nfd,
            alternative_forms={"name-normalized": [nfc]},
        )
        verdict = classify(exp, [node("text", nfc)])
        self.assertEqual(verdict.verdict, "FAIL")
        self.assertEqual(verdict.prohibited_outcome, "name-normalized")
        self.assertEqual(verdict.observed_name, nfc)

    def test_name_is_shaped_glyphs_matches_the_precommitted_ligature_form(self):
        # F-A's case: the `ff` ligature collapses, so a wrong tree drops a
        # letter relative to the source string.
        exp = expectation(
            "Allegro affettuoso — al fine",
            alternative_forms={"name-is-shaped-glyphs": ["Allegro afettuoso — al fne"]},
        )
        verdict = classify(exp, [node("text", "Allegro afettuoso — al fne")])
        self.assertEqual(verdict.verdict, "FAIL")
        self.assertEqual(verdict.prohibited_outcome, "name-is-shaped-glyphs")

    def test_name_drops_unresolved_codepoints_matches_the_precommitted_form(self):
        # F-C's case: U+0627 is covered by no declared face and is dropped.
        exp = expectation(
            "Coro ا",
            alternative_forms={"name-drops-unresolved-codepoints": ["Coro "]},
        )
        verdict = classify(exp, [node("static", "Coro ")])
        self.assertEqual(verdict.verdict, "FAIL")
        self.assertEqual(verdict.prohibited_outcome, "name-drops-unresolved-codepoints")

    def test_a_correct_name_does_not_spuriously_match_an_alternative_form(self):
        """Mutation guard: if alternative-form matching ran before the exact
        match, a correct name would risk matching an alternative_forms entry
        by accident and wrongly FAIL."""
        exp = expectation(
            "Coro ا",
            alternative_forms={"name-drops-unresolved-codepoints": ["Coro "]},
        )
        verdict = classify(exp, [node("static", "Coro ا")])
        self.assertEqual(verdict.verdict, "PASS")


class VisualOrderDiagnosis(unittest.TestCase):
    def test_visual_order_concatenation_is_named_specifically(self):
        # F-D's designed trap: a tree walking visual runs left to right
        # reverses the embedded RTL segment's codepoint order.
        exp = expectation(
            "Allegro אבג con brio",
            visual_order_name="Allegro גבא con brio",
        )
        run = node(
            "frame",
            "",
            children=[node("text", "Allegro "), node("text", "גבא"), node("text", " con brio")],
        )
        verdict = classify(exp, [run])
        self.assertEqual(verdict.verdict, "FAIL")
        self.assertEqual(verdict.prohibited_outcome, VISUAL_ORDER_TRAP)
        self.assertEqual(verdict.observed_name, exp["visual_order_name"])

    def test_visual_order_diagnosis_is_not_reported_as_a_generic_mismatch(self):
        """Mutation guard: if the visual-order check were deleted, this
        would still FAIL but with `prohibited_outcome=None` (the generic
        fallback) instead of the specific diagnosis — asserting the exact
        name, not just FAIL, is what catches that."""
        exp = expectation(
            "Allegro אבג con brio",
            visual_order_name="Allegro גבא con brio",
        )
        run = node(
            "frame",
            "",
            children=[node("text", "Allegro "), node("text", "גבא"), node("text", " con brio")],
        )
        verdict = classify(exp, [run])
        self.assertIsNotNone(verdict.prohibited_outcome)
        self.assertNotEqual(verdict.prohibited_outcome, "absent-from-tree")
        self.assertNotEqual(verdict.prohibited_outcome, "name-empty")


class ProhibitedRole(unittest.TestCase):
    def test_a_prohibited_role_fails_even_with_the_exact_name(self):
        exp = expectation("Allegro")
        verdict = classify(exp, [node("canvas", "Allegro")])
        self.assertEqual(verdict.verdict, "FAIL")
        self.assertEqual(verdict.observed_role, "canvas")
        # This is a role-vocabulary failure (§8.2), not one of the five
        # name-transformation PROHIBITED_OUTCOMES (§8.3) — it must not be
        # reported as one.
        self.assertIsNone(verdict.prohibited_outcome)
        self.assertEqual(
            verdict.reason,
            "a node's name matches expected_name byte-for-byte, but its role 'canvas' is in the "
            "at-spi2 prohibited set (no accepted-role node also carries it)",
        )

    def test_an_accepted_role_with_the_exact_name_is_not_penalized(self):
        """Mutation guard: confirms the previous test is actually exercising
        the role check, not some other reason that name would fail."""
        exp = expectation("Allegro")
        verdict = classify(exp, [node("label", "Allegro")])
        self.assertEqual(verdict.verdict, "PASS")

    def test_an_unlisted_role_carrying_the_exact_name_is_not_absent_from_tree(self):
        """A role outside `accepted_roles | prohibited_roles` (e.g. `push
        button`) is never a text-*candidate* for the composition/single-node
        role checks — `_flatten_candidates`/`_subtree_contributors` exclude
        it. But a node under that role can still carry the run's exact
        text, and §8.3's absent-from-tree ("the default outcome for a
        toolkit that draws to a canvas and stops") does not describe that:
        the run *is* in the tree. This must FAIL naming the actual observed
        role, not report absent-from-tree — conflating "wrong role" with
        "nothing there at all" would hide evidence a real candidate
        produced."""
        exp = expectation("Allegro")
        verdict = classify(exp, [node("push button", "Allegro")])
        self.assertEqual(verdict.verdict, "FAIL")
        self.assertNotEqual(verdict.prohibited_outcome, "absent-from-tree")
        self.assertEqual(verdict.observed_role, "push button")
        self.assertEqual(verdict.observed_name, "Allegro")

    def test_a_truly_empty_tree_is_still_absent_from_tree(self):
        """The companion case to the one above, pinned side by side so a
        regression that merges the two back together (e.g. by making the
        new unlisted-role scan fire unconditionally) is caught: with
        nothing in the tree at all, the outcome must still be
        `absent-from-tree`."""
        exp = expectation("Allegro")
        verdict = classify(exp, [])
        self.assertEqual(verdict.verdict, "FAIL")
        self.assertEqual(verdict.prohibited_outcome, "absent-from-tree")

    def test_an_unlisted_role_carrying_an_alternative_form_is_also_not_absent_from_tree(self):
        """The whole-tree scan must check precommitted alternative forms
        too, not only `expected_name` — a candidate that shaped the name
        wrong *and* exposed it under an unlisted role has still put the
        (wrong) text in the tree, which is a different, more specific,
        finding than "nothing is there."""
        exp = expectation(
            "Coro ا",
            alternative_forms={"name-drops-unresolved-codepoints": ["Coro "]},
        )
        verdict = classify(exp, [node("push button", "Coro ")])
        self.assertEqual(verdict.verdict, "FAIL")
        self.assertNotEqual(verdict.prohibited_outcome, "absent-from-tree")
        self.assertEqual(verdict.observed_role, "push button")
        self.assertEqual(verdict.observed_name, "Coro ")

    def test_an_unlisted_role_carrying_the_visual_order_form_is_also_not_absent_from_tree(self):
        exp = expectation(
            "Allegro אבג con brio",
            visual_order_name="Allegro גבא con brio",
        )
        verdict = classify(exp, [node("push button", "Allegro גבא con brio")])
        self.assertEqual(verdict.verdict, "FAIL")
        self.assertNotEqual(verdict.prohibited_outcome, "absent-from-tree")
        self.assertEqual(verdict.observed_role, "push button")

    def test_an_unlisted_role_carrying_an_unrelated_name_is_still_absent_from_tree(self):
        """Mutation guard: the whole-tree scan must only match a name
        against `expected_name`/alternative forms/`visual_order_name` — a
        node with an unlisted role and completely unrelated text must not
        rescue the verdict away from absent-from-tree either."""
        exp = expectation("Allegro")
        verdict = classify(exp, [node("push button", "something unrelated")])
        self.assertEqual(verdict.verdict, "FAIL")
        self.assertEqual(verdict.prohibited_outcome, "absent-from-tree")


class MultipleFormsPerOutcome(unittest.TestCase):
    """O2: one outcome can carry more than one precommitted rendering —
    `round2-a11y-oracle` gives `name-is-shaped-glyphs` both a
    cluster-collapse form and a ligature presentation-form substitution for
    F-A. Either one observed must classify the same outcome."""

    def _f_a_like_expectation(self):
        return expectation(
            "Allegro affettuoso — al fine",
            alternative_forms={
                "name-is-shaped-glyphs": [
                    "Allegro afettuoso — al fne",
                    "Allegro a\N{LATIN SMALL LIGATURE FF}ettuoso — al \N{LATIN SMALL LIGATURE FI}ne",
                ]
            },
        )

    def test_the_cluster_collapse_form_classifies(self):
        verdict = classify(
            self._f_a_like_expectation(), [node("text", "Allegro afettuoso — al fne")]
        )
        self.assertEqual(verdict.verdict, "FAIL")
        self.assertEqual(verdict.prohibited_outcome, "name-is-shaped-glyphs")

    def test_the_presentation_form_classifies_the_same_outcome(self):
        presentation = (
            "Allegro a\N{LATIN SMALL LIGATURE FF}ettuoso — al \N{LATIN SMALL LIGATURE FI}ne"
        )
        verdict = classify(self._f_a_like_expectation(), [node("text", presentation)])
        self.assertEqual(verdict.verdict, "FAIL")
        self.assertEqual(verdict.prohibited_outcome, "name-is-shaped-glyphs")

    def test_a_third_string_matches_neither_form(self):
        """Mutation guard: confirms the two tests above are matching a
        specific listed string, not merely "anything different from
        expected_name" — if list matching degenerated to that, this would
        wrongly report name-is-shaped-glyphs too."""
        verdict = classify(
            self._f_a_like_expectation(),
            [node("text", "something else entirely")],
        )
        self.assertNotEqual(verdict.prohibited_outcome, "name-is-shaped-glyphs")


class SubtreeScopedComposition(unittest.TestCase):
    """B1: composition is scored against one run subtree's own descendants,
    never the whole application flattened into one list. These are the two
    cases the coordinator reproduced against the pre-B1 flat classifier:

    - `[("canvas", "Coro "), ("text", "אבג")]` wrongly PASSed.
    - `[("text", "Coro "), ("text", "אבג"), ("label", "MyApp Window")]`
      wrongly FAILed, even though `label` is an accepted at-spi2 role that
      every real application's window frame carries, elsewhere in the tree.
    """

    def _f_b_like_expectation(self):
        return expectation("Coro אבג")

    def test_a_prohibited_role_sibling_in_the_same_run_subtree_fails_with_the_role_named(self):
        """The false-PASS case (B1's first reproduction), expressed as a
        real tree: `canvas` and `text` are siblings under one run subtree —
        together they still spell out expected_name byte-for-byte, but a
        `canvas` contributed to it, which must FAIL, naming `canvas`, not
        PASS."""
        run = node("frame", "", children=[node("canvas", "Coro "), node("text", "אבג")])
        verdict = classify(self._f_b_like_expectation(), [run])
        self.assertEqual(verdict.verdict, "FAIL")
        self.assertEqual(verdict.observed_role, "canvas")
        self.assertEqual(verdict.observed_name, "Coro אבג")
        self.assertIn("canvas", verdict.reason)
        self.assertIn("otherwise-correct composition", verdict.reason)
        # Not one of the five PROHIBITED_OUTCOMES names — this is a §8.2
        # role-vocabulary failure inside a composition, not a §8.3 name
        # transformation.
        self.assertIsNone(verdict.prohibited_outcome)

    def test_an_unrelated_accepted_role_node_elsewhere_does_not_poison_a_correct_composition(self):
        """The false-FAIL case (B1's second reproduction), expressed as a
        real tree: the run's own two text nodes are correctly grouped under
        their own subtree; an unrelated `label` (an *accepted* at-spi2 role
        — every real window frame carries one) sits elsewhere in the same
        application. The label must not be able to corrupt the run's own,
        otherwise-correct, composition into a FAIL."""
        application = node(
            "frame",
            "",
            children=[
                node("group", "", children=[node("text", "Coro "), node("text", "אבג")]),
                node("label", "MyApp Window"),
            ],
        )
        verdict = classify(self._f_b_like_expectation(), [application])
        self.assertEqual(verdict.verdict, "PASS")
        self.assertIsNone(verdict.prohibited_outcome)
        self.assertEqual(verdict.observed_name, "Coro אבג")

    def test_the_legitimate_accepted_role_split_still_passes_when_it_is_the_whole_tree(self):
        """Sanity companion to the two reproductions above: a run correctly
        split across two accepted-role text nodes, with nothing else in the
        tree at all, must still PASS — B1's fix must not have become so
        conservative that it stopped recognizing the ordinary case."""
        run = node("paragraph", "", children=[node("text", "Coro "), node("text", "אבג")])
        verdict = classify(self._f_b_like_expectation(), [run])
        self.assertEqual(verdict.verdict, "PASS")
        self.assertIsNone(verdict.prohibited_outcome)

    def test_a_prohibited_contributor_is_named_even_when_a_correct_subtree_exists_elsewhere(self):
        """The composition scan must not let a PASS found in one subtree
        erase evidence of a bad contributor found in *another* — but it must
        still prefer reporting the PASS overall, since a candidate that gets
        it right anywhere in a legitimate run subtree has satisfied §8.1.
        This test pins the reverse: when NO subtree passes cleanly, the
        reported reason must name the actual bad contributor, not a generic
        mismatch."""
        run = node(
            "frame",
            "",
            children=[node("canvas", "Coro "), node("text", "אבג")],
        )
        verdict = classify(self._f_b_like_expectation(), [run])
        self.assertEqual(verdict.verdict, "FAIL")
        self.assertEqual(verdict.observed_role, "canvas")
        self.assertNotIn("no node or composition matched", verdict.reason)

    def test_a_composition_that_silently_dropped_a_prohibited_contributor_would_be_wrong(self):
        """Guards the specific failure mode B1 warns against: a subtree must
        not PASS by concatenating only its *accepted*-role contributors and
        ignoring a prohibited one. Here, dropping the `canvas` node would
        leave just `"אבג"`, which does not equal expected_name either — so
        this also confirms the concatenation includes every contributor's
        name, not a filtered subset, before the role check ever runs."""
        run = node("frame", "", children=[node("canvas", "Coro "), node("text", "אבג")])
        verdict = classify(self._f_b_like_expectation(), [run])
        # If contributors had been filtered to accepted-only before
        # concatenating, the concat would be "אבג" (not expected_name), and
        # this subtree would be silently skipped rather than FAILed with a
        # named reason — falling through to a *weaker* diagnosis than the
        # sharp one B1 requires.
        self.assertEqual(verdict.verdict, "FAIL")
        self.assertIn("canvas", verdict.reason)


class ContainerNamingDoesNotChangeBlame(unittest.TestCase):
    """An empty-named structural wrapper (a `frame` around the real
    contributors, exposing no name of its own) must never be blamed for a
    bad composition. It contributes zero bytes to the concatenation, so it
    cannot be what made the composition wrong; blaming it hides the actual
    offender — here, a *prohibited*-role `canvas` that carried half the
    run, which is the real §8.2 violation the report exists to name.

    Same tree content in all three shapes below; only the container's own
    name (or its absence) differs. All three must name `canvas`."""

    def _f_b_like_expectation(self):
        return expectation("Coro אבג")

    def test_canvas_and_text_inside_an_unnamed_frame_blames_canvas(self):
        tree = node(
            "application",
            "p",
            children=[node("frame", "", children=[node("canvas", "Coro "), node("text", "אבג")])],
        )
        verdict = classify(self._f_b_like_expectation(), [tree])
        self.assertEqual(verdict.verdict, "FAIL")
        self.assertEqual(verdict.observed_role, "canvas")

    def test_canvas_and_text_inside_a_named_frame_blames_canvas(self):
        tree = node(
            "application",
            "p",
            children=[
                node("frame", "MyApp", children=[node("canvas", "Coro "), node("text", "אבג")])
            ],
        )
        verdict = classify(self._f_b_like_expectation(), [tree])
        self.assertEqual(verdict.verdict, "FAIL")
        self.assertEqual(verdict.observed_role, "canvas")

    def test_canvas_and_text_as_direct_siblings_blames_canvas(self):
        tree = node(
            "application",
            "p",
            children=[node("canvas", "Coro "), node("text", "אבג")],
        )
        verdict = classify(self._f_b_like_expectation(), [tree])
        self.assertEqual(verdict.verdict, "FAIL")
        self.assertEqual(verdict.observed_role, "canvas")

    def test_an_unlisted_but_not_prohibited_contributor_is_still_named_when_no_prohibited_one_exists(
        self,
    ):
        """The weaker fallback branch stays reachable: with no
        prohibited-role contributor at all, an unlisted-role one is still
        named (not silently dropped just because it is the weaker case).
        Nested under an unnamed wrapper, so this test also isolates the
        empty-name exclusion on its own: with no prohibited contributor
        present, the "prefer prohibited" rule cannot be what saves this
        case from blaming the wrapper — only excluding the empty-named
        `frame` from the contributor set can. A mutation that deleted the
        empty-name exclusion (but kept the prohibited-preference) would
        wrongly blame `frame` here, even though the same mutation happens
        to survive the two `blames_canvas` tests above (where a prohibited
        `canvas` is also present and the preference rule alone rescues
        them)."""
        tree = node(
            "application",
            "p",
            children=[
                node(
                    "frame", "", children=[node("push button", "Coro "), node("text", "אבג")]
                )
            ],
        )
        verdict = classify(self._f_b_like_expectation(), [tree])
        self.assertEqual(verdict.verdict, "FAIL")
        self.assertEqual(verdict.observed_role, "push button")
        self.assertIn("neither accepted nor prohibited", verdict.reason)

    def test_a_prohibited_contributor_is_preferred_over_an_unlisted_one_listed_first(self):
        """Isolates the "prefer prohibited" rule specifically, with no
        empty-named node anywhere in the tree: an unlisted-role
        (`push button`) contributor is listed *before* a prohibited-role
        (`canvas`) one, both non-empty-named. Naming "the first non-accepted
        contributor" (no preference) would wrongly blame `push button`; only
        the explicit prohibited-preference blames `canvas`."""
        tree = node(
            "frame",
            "",
            children=[node("push button", "Coro "), node("canvas", "אבג")],
        )
        verdict = classify(self._f_b_like_expectation(), [tree])
        self.assertEqual(verdict.verdict, "FAIL")
        self.assertEqual(verdict.observed_role, "canvas")
        self.assertIn("prohibited", verdict.reason)

    def test_a_correct_split_run_nested_under_an_unnamed_container_still_passes(self):
        """Confirms excluding empty-named nodes does not disturb a
        legitimate PASS: the concatenation is unchanged whether the
        empty-named wrapper is included or excluded (it contributes zero
        bytes either way), so a correct split run nested under one must
        still pass."""
        tree = node(
            "application",
            "p",
            children=[node("frame", "", children=[node("text", "Coro "), node("text", "אבג")])],
        )
        verdict = classify(self._f_b_like_expectation(), [tree])
        self.assertEqual(verdict.verdict, "PASS")


class ExactNamePrecedence(unittest.TestCase):
    """C1: exact-name matching must evaluate every node before deciding,
    never return on the first match — an accepted-role node carrying
    expected_name wins regardless of where in the tree it sits, even when a
    prohibited-role node carrying the *same* exact name is listed first."""

    @staticmethod
    def _forest(canvas_first):
        canvas = node("canvas", "Coro אבג")
        text = node("text", "Coro אבג")
        return [canvas, text] if canvas_first else [text, canvas]

    def test_prohibited_role_node_listed_first_still_passes(self):
        exp = expectation("Coro אבג")
        verdict = classify(exp, self._forest(canvas_first=True))
        self.assertEqual(verdict.verdict, "PASS")
        self.assertEqual(verdict.observed_role, "text")
        self.assertIsNone(verdict.prohibited_outcome)

    def test_accepted_role_node_listed_first_still_passes(self):
        exp = expectation("Coro אבג")
        verdict = classify(exp, self._forest(canvas_first=False))
        self.assertEqual(verdict.verdict, "PASS")
        self.assertEqual(verdict.observed_role, "text")
        self.assertIsNone(verdict.prohibited_outcome)

    def test_both_orderings_of_the_forest_produce_the_same_verdict(self):
        """The direct C1 reproduction: the coordinator measured opposite
        verdicts for the two orderings of this exact forest. Pinned here as
        one assertion comparing both `classify` calls, not two independently
        hand-written expectations that could each be individually wrong in
        the same direction."""
        exp = expectation("Coro אבג")
        v_canvas_first = classify(exp, self._forest(canvas_first=True))
        v_text_first = classify(exp, self._forest(canvas_first=False))
        self.assertEqual(v_canvas_first.verdict, v_text_first.verdict)
        self.assertEqual(v_canvas_first.observed_role, v_text_first.observed_role)
        self.assertEqual(v_canvas_first.prohibited_outcome, v_text_first.prohibited_outcome)
        self.assertEqual(v_canvas_first.verdict, "PASS")

    def test_without_any_accepted_role_match_the_prohibited_one_still_fails(self):
        """Mutation guard: confirms the PASSes above happen *because* an
        accepted-role match exists, not because exact-name matching became
        unconditional PASS — with only the prohibited-role node present,
        this must still FAIL."""
        exp = expectation("Coro אבג")
        verdict = classify(exp, [node("canvas", "Coro אבג")])
        self.assertEqual(verdict.verdict, "FAIL")
        self.assertEqual(verdict.observed_role, "canvas")


class SubtreeScopedDiagnoses(unittest.TestCase):
    """C2: the alternative-form and visual-order diagnoses are scored per
    subtree, exactly like composition (B1) — an unrelated accepted-role node
    elsewhere in the application (e.g. a window `label`) must not poison a
    legitimate subtree's diagnosis into a generic mismatch."""

    def test_visual_order_trap_is_found_despite_an_unrelated_window_label(self):
        """The direct C2 reproduction: F-D's designed visual-order trap,
        with an unrelated `label` elsewhere in the application."""
        exp = expectation(
            "Allegro אבג con brio",
            visual_order_name="Allegro גבא con brio",
        )
        application = node(
            "application",
            "p",
            children=[
                node("label", "MyApp Window"),
                node(
                    "frame",
                    "",
                    children=[
                        node("text", "Allegro "),
                        node("text", "גבא"),
                        node("text", " con brio"),
                    ],
                ),
            ],
        )
        verdict = classify(exp, [application])
        self.assertEqual(verdict.verdict, "FAIL")
        self.assertEqual(verdict.prohibited_outcome, VISUAL_ORDER_TRAP)
        self.assertNotIn("no node or composition matched", verdict.reason)

    def test_alternative_form_composition_is_found_despite_an_unrelated_window_label(self):
        """The same poisoning bug, for an alternative-form composition (not
        visual-order) — the ligature-collapse form split across two text
        nodes, so this exercises the *subtree-concatenation* alt-form path
        specifically, not the already-order-independent single-node one."""
        exp = expectation(
            "Allegro affettuoso — al fine",
            alternative_forms={"name-is-shaped-glyphs": ["Allegro afettuoso — al fne"]},
        )
        application = node(
            "application",
            "p",
            children=[
                node("label", "MyApp Window"),
                node(
                    "frame",
                    "",
                    children=[
                        node("text", "Allegro afettuoso — al "),
                        node("text", "fne"),
                    ],
                ),
            ],
        )
        verdict = classify(exp, [application])
        self.assertEqual(verdict.verdict, "FAIL")
        self.assertEqual(verdict.prohibited_outcome, "name-is-shaped-glyphs")
        self.assertNotIn("no node or composition matched", verdict.reason)

    def test_without_the_unrelated_label_the_same_composition_still_matches(self):
        """Mutation guard: confirms the two tests above are really about the
        label not poisoning the result, not about some other property of
        the tree shape — remove the label and the same diagnosis must still
        fire."""
        exp = expectation(
            "Allegro אבג con brio",
            visual_order_name="Allegro גבא con brio",
        )
        frame = node(
            "frame",
            "",
            children=[node("text", "Allegro "), node("text", "גבא"), node("text", " con brio")],
        )
        verdict = classify(exp, [frame])
        self.assertEqual(verdict.prohibited_outcome, VISUAL_ORDER_TRAP)


class UnlistedRoleComposition(unittest.TestCase):
    """C3: text composed across two or more unlisted-role descendants must
    not be misreported as absent-from-tree — the same per-subtree
    composition scoring applied to roles in neither `accepted_roles` nor
    `prohibited_roles`."""

    def test_two_unlisted_role_nodes_composing_the_exact_name_is_not_absent(self):
        """The direct C3 reproduction: two `push button` nodes whose
        concatenation is the run's exact text."""
        exp = expectation("Coro אבג")
        application = node(
            "application",
            "p",
            children=[node("push button", "Coro "), node("push button", "אבג")],
        )
        verdict = classify(exp, [application])
        self.assertEqual(verdict.verdict, "FAIL")
        self.assertNotEqual(verdict.prohibited_outcome, "absent-from-tree")
        self.assertEqual(verdict.observed_name, "Coro אבג")

    def test_unlisted_role_composition_with_unrelated_text_is_still_absent(self):
        """The composition-scan analogue of the single-node absent-from-tree
        guard: two unlisted-role nodes whose concatenation is *not* the
        run's text, an alternative form, or visual_order_name, must still
        classify as genuine absence — the scan must not over-fire just
        because *some* unlisted-role composition exists."""
        exp = expectation("Coro אבג")
        application = node(
            "application",
            "p",
            children=[node("push button", "something"), node("push button", "unrelated")],
        )
        verdict = classify(exp, [application])
        self.assertEqual(verdict.verdict, "FAIL")
        self.assertEqual(verdict.prohibited_outcome, "absent-from-tree")


class AbsentFromTreeVsMisorderedFragments(unittest.TestCase):
    """User ruling, following C3: a permutation probe found that reversing
    the same two contributors gave `absent-from-tree` under unlisted roles
    but a plain generic mismatch under accepted roles — two different named
    outcomes for the same underlying defect (contributors in the wrong
    order). Three cases, side by side, each asserting its own distinct
    outcome, so a future change cannot silently re-merge them.

    **Behaviour change, reported explicitly (not adjusted quietly), from the
    reorder that moved source-bearing detection to run before absence/
    name-empty (the very next ruling in this same sequence):** case 1 below
    used to assert "unchanged, a generic composition FAIL" — that was true
    only because the fragment scan, at the time, ran solely inside the
    `flat_candidates`-empty branch and so never even looked at accepted-role
    contributors. Once source-bearing detection (fragments included) was
    unified to run over *every* role unconditionally, the identical
    reversed-`text` case is now *also* caught by the fragment scan, with a
    more specific message than the old generic mismatch — which is the
    intended, uniform consequence of "misordered source fragments ->
    composition/role failure, never absence" applying without a role
    exception. Nothing about *this* file's tests silently changed; the
    updated assertion below is that report.

    1. reversed **accepted**-role contributors — a role/composition FAIL
       (fragment-scan diagnosis, naming the fragments), never
       `absent-from-tree`;
    2. reversed **unlisted**-role contributors — the original fix: a
       role/composition FAIL, never `absent-from-tree`;
    3. the true-absence control — nothing resembling the run's text
       anywhere — still reaches `absent-from-tree`, proving the fix
       narrowed the bug without making the outcome unreachable.
    """

    def test_reversed_accepted_role_contributors_are_a_composition_failure_not_absence(self):
        """Behaviour change (see class docstring): this used to assert a
        generic mismatch (`prohibited_outcome=None`, "no node or
        composition matched..."). It now asserts the fragment-scan
        diagnosis — still `prohibited_outcome=None` (a §8.2 role/composition
        failure, not a §8.3 PROHIBITED_OUTCOMES name), but a more specific
        reason, because the fragment scan is no longer gated to unlisted
        roles only."""
        exp = expectation("Coro אבג")
        run = node("frame", "", children=[node("text", "אבג"), node("text", "Coro ")])
        verdict = classify(exp, [run])
        self.assertEqual(verdict.verdict, "FAIL")
        self.assertIsNone(verdict.prohibited_outcome)
        self.assertNotEqual(verdict.prohibited_outcome, "absent-from-tree")
        self.assertIn("role/composition failure", verdict.reason)

    def test_reversed_unlisted_role_contributors_are_a_role_failure_not_absence(self):
        """Item 2, the fix itself — the coordinator's exact reproduction:
        the identical shape, under unlisted roles, must NOT be
        `absent-from-tree`. The text is genuinely present; only the order
        (and the role) is wrong."""
        exp = expectation("Coro אבג")
        application = node(
            "application",
            "p",
            children=[node("push button", "אבג"), node("push button", "Coro ")],
        )
        verdict = classify(exp, [application])
        self.assertEqual(verdict.verdict, "FAIL")
        self.assertNotEqual(verdict.prohibited_outcome, "absent-from-tree")
        # Not a §8.3 PROHIBITED_OUTCOMES name either — this is the §8.2
        # role/composition failure category, the same as C3's other cases.
        self.assertIsNone(verdict.prohibited_outcome)

    def test_true_absence_is_still_reachable(self):
        """Item 1, the control: with nothing resembling the run's text
        anywhere, `absent-from-tree` must still fire — proving the fix
        narrowed the bug rather than making the outcome unreachable."""
        exp = expectation("Coro אבג")
        verdict = classify(exp, [])
        self.assertEqual(verdict.verdict, "FAIL")
        self.assertEqual(verdict.prohibited_outcome, "absent-from-tree")


class SourceBearingFragmentGuards(unittest.TestCase):
    """Prove `_is_source_bearing_fragment` (C3's fix) is not loose enough to
    make `absent-from-tree` unreachable in practice — the user's own stated
    concern: "a rule loose enough that an application's ordinary window
    label counts as a fragment would make absent-from-tree unreachable in
    practice, which is a worse failure than the one being fixed." Each
    guard below is the specific scenario that concern describes."""

    def test_a_single_shared_character_is_not_a_fragment(self):
        """A one-character coincidence — "o" appears in both "Coro" and
        almost any ordinary English text — must not count; this is exactly
        the case the length-2 floor exists to exclude."""
        self.assertFalse(_is_source_bearing_fragment("o", {"Coro אבג"}))

    def test_an_ordinary_window_label_is_not_a_fragment(self):
        """The user's own example, direct: a whole, realistic window title
        is longer than (and unrelated to) the run's short text, so it can
        never be a literal substring of it."""
        self.assertFalse(_is_source_bearing_fragment("MyApp Window", {"Coro אבג"}))

    def test_a_whitespace_only_name_is_not_a_fragment(self):
        self.assertFalse(_is_source_bearing_fragment("   ", {"Coro אבג"}))

    def test_an_empty_name_is_not_a_fragment(self):
        self.assertFalse(_is_source_bearing_fragment("", {"Coro אבג"}))

    def test_a_two_character_real_fragment_does_count(self):
        """Anchors the floor at exactly two characters, not three or more —
        confirms the guards above are testing the length-1 boundary
        specifically, not merely "short strings never match"."""
        self.assertTrue(_is_source_bearing_fragment("בג", {"Coro אבג"}))

    def test_an_ordinary_window_label_does_not_rescue_a_tree_from_absence_end_to_end(self):
        """The end-to-end version of the guard above: a real, unrelated,
        realistic window label under an unlisted role, with nothing else in
        the tree, must still classify as absent-from-tree."""
        exp = expectation("Coro אבג")
        application = node(
            "application",
            "p",
            # An unlisted role, not "label" (which is an accepted at-spi2
            # role and would exit the C3 branch this test is about via the
            # ordinary accepted-role path instead).
            children=[node("push button", "MyApp Window")],
        )
        verdict = classify(exp, [application])
        self.assertEqual(verdict.verdict, "FAIL")
        self.assertEqual(verdict.prohibited_outcome, "absent-from-tree")


class FCTwoSegmentComposition(unittest.TestCase):
    """D1/D2 regression group, required together — all four reproduce
    against F-C's real shape (`"Coro "` + `"ا"`, with `"Coro "` also being
    F-C's own precommitted `name-drops-unresolved-codepoints` form) and must
    all hold simultaneously:

        unlisted "ا" for F-C                          -> NOT absent-from-tree
        unrelated single ASCII "o"                    -> still NOT source-bearing
        F-C accepted split ["Coro ", "ا"]              -> PASS
        F-C lone accepted node named exactly "Coro "  -> still name-drops-unresolved-codepoints

    The first two prove D1 (an unresolved segment can be a single character,
    which the length-2 substring rule alone cannot catch, but the
    coincidence guard must still hold); the last two prove D2 (a legitimate
    two-node split now PASSes despite the first segment alone matching a
    precommitted alternative form, and the single-node case — which has no
    composition to find — still names that outcome exactly as before).
    """

    def _f_c_like_expectation(self):
        return expectation(
            "Coro ا",
            alternative_forms={"name-drops-unresolved-codepoints": ["Coro "]},
            source_atoms=["Coro ", "ا"],
        )

    def test_unlisted_role_carrying_f_cs_unresolved_segment_is_not_absent_from_tree(self):
        """D1, the reported finding: F-C's unresolved segment `ا` is a
        single character — below `_is_source_bearing_fragment`'s length-2
        floor — but it is still a precommitted `source_atoms` entry, so a
        node carrying it under an unlisted role must be a role/composition
        failure, never absence. §8.3: "the accessibility tree carries the
        text, not the ink"."""
        exp = self._f_c_like_expectation()
        verdict = classify(exp, [node("push button", "ا")])
        self.assertEqual(verdict.verdict, "FAIL")
        self.assertNotEqual(verdict.prohibited_outcome, "absent-from-tree")
        self.assertEqual(verdict.observed_role, "push button")

    def test_an_unrelated_single_ascii_character_is_still_not_source_bearing(self):
        """Mutation guard, D1: confirms the atom-matching path is additive
        and precommitted, not a blanket "any single character counts" rule
        — an unrelated stray `"o"` (present in `"Coro"` only by coincidence,
        and not a `source_atoms` entry) must still not rescue the tree from
        absence, exactly as `_is_source_bearing_fragment`'s own coincidence
        guard already requires on its own."""
        exp = self._f_c_like_expectation()
        verdict = classify(exp, [node("push button", "o")])
        self.assertEqual(verdict.verdict, "FAIL")
        self.assertEqual(verdict.prohibited_outcome, "absent-from-tree")

    def test_the_legitimate_two_node_split_passes(self):
        """D2, the fix itself — the coordinator's exact reproduction: two
        ACCEPTED-role text nodes, one per direction run (exactly what §8.1
        permits: "a tree that exposes one text node per direction run is
        not wrong"), must PASS even though the first segment alone happens
        to equal F-C's own precommitted `name-drops-unresolved-codepoints`
        form."""
        exp = self._f_c_like_expectation()
        run = node("frame", "", children=[node("text", "Coro "), node("text", "ا")])
        verdict = classify(exp, [run])
        self.assertEqual(verdict.verdict, "PASS")
        self.assertIsNone(verdict.prohibited_outcome)

    def test_a_lone_accepted_node_named_exactly_coro_is_still_the_named_outcome(self):
        """The required guard proving D2's fix did not simply disable the
        alternative-form diagnosis: with no second node, there is no
        composition to find, so a single `text:"Coro "` node must still be
        classified by name, exactly as before D2."""
        exp = self._f_c_like_expectation()
        verdict = classify(exp, [node("text", "Coro ")])
        self.assertEqual(verdict.verdict, "FAIL")
        self.assertEqual(verdict.prohibited_outcome, "name-drops-unresolved-codepoints")


class AbsentFromTreeVsNameEmpty(unittest.TestCase):
    """User ruling: `absent-from-tree` must be gated on *no source-bearing
    text existing anywhere*, not on *no accepted-or-prohibited-role
    candidate existing* — the latter gate is nearly always false for a real
    application (a window `label` alone defeats it), which made
    `absent-from-tree` — §8.3's own words, "the one this check will most
    likely actually catch" — effectively unreachable in practice.

    The pinned precedence:

    1. Source-bearing text or a precommitted form present anywhere ->
       classify its name/role/composition outcome. This runs first, over
       the whole forest, any role — before any absence/empty-name
       determination.
    2. Otherwise, `name-empty` only when both hold: at least one
       **accepted**-role candidate node exists, and every
       accepted-or-prohibited-role candidate's name is empty.
    3. Every other no-source-bearing case -> `absent-from-tree`.

    The intended taxonomy, and the required regression lock: four cases,
    side by side, so a future change cannot re-merge them.

        unrelated UI text only              -> absent-from-tree
        empty prohibited canvas only        -> absent-from-tree
        empty accepted text/label node      -> name-empty
        misordered source fragments         -> composition/role failure, never absence

    The distinction being preserved: `name-empty` means an attempted
    static-text exposure without a name; `absent-from-tree` covers
    drawing-only or unrelated trees. A prohibited-role empty node is not
    "wearing a role" in §8.3's sense — it is the draw-and-stop case.
    """

    def test_unrelated_ui_text_only_is_absent_from_tree(self):
        """The coordinator's exact reproduction: ordinary application
        chrome — a button, a window label — none of it related to the run.
        Every real application has role-listed nodes like this, which is
        exactly why the old "no candidate exists anywhere" gate made
        `absent-from-tree` nearly unreachable."""
        exp = expectation("Coro אבג")
        application = node(
            "application",
            "p",
            children=[node("push button", "Save"), node("label", "MyApp Window")],
        )
        verdict = classify(exp, [application])
        self.assertEqual(verdict.verdict, "FAIL")
        self.assertEqual(verdict.prohibited_outcome, "absent-from-tree")

    def test_empty_prohibited_canvas_only_is_absent_from_tree(self):
        """The case that matters most (§8.3's own words): a toolkit that
        drew to a canvas and stopped. A `canvas` node exposing no name is
        not "wearing a role" in §8.3's name-empty sense — with no
        accepted-role candidate anywhere in the tree, this is the
        draw-and-stop case, `absent-from-tree`, not `name-empty`."""
        exp = expectation("Coro אבג")
        application = node("frame", "MyApp", children=[node("canvas", "")])
        verdict = classify(exp, [application])
        self.assertEqual(verdict.verdict, "FAIL")
        self.assertEqual(verdict.prohibited_outcome, "absent-from-tree")

    def test_empty_accepted_text_node_is_name_empty(self):
        """The companion case: an *accepted*-role node attempting to expose
        static text, but with no name — this is the genuine name-empty
        case, "absence wearing a role"."""
        exp = expectation("Coro אבג")
        application = node("frame", "MyApp", children=[node("label", "")])
        verdict = classify(exp, [application])
        self.assertEqual(verdict.verdict, "FAIL")
        self.assertEqual(verdict.prohibited_outcome, "name-empty")

    def test_misordered_source_fragments_are_a_composition_failure_not_absence_or_empty(self):
        """The fourth leg: fragments of the run's actual text, present but
        in the wrong order, must never be classified as absence or
        name-empty — a composition/role failure, per item 1's precedence
        over items 2 and 3."""
        exp = expectation("Coro אבג")
        application = node(
            "application",
            "p",
            children=[node("push button", "אבג"), node("push button", "Coro ")],
        )
        verdict = classify(exp, [application])
        self.assertEqual(verdict.verdict, "FAIL")
        self.assertNotEqual(verdict.prohibited_outcome, "absent-from-tree")
        self.assertNotEqual(verdict.prohibited_outcome, "name-empty")

    def test_an_accepted_node_with_real_text_alongside_an_empty_one_is_not_name_empty(self):
        """Mutation guard: `name-empty` requires *every* candidate to be
        empty, not merely *an* accepted-role candidate existing — an
        accepted node with real (if unrelated) text sitting alongside an
        empty one must not trigger name-empty, since not every
        text-candidate node is actually empty."""
        exp = expectation("Coro אבג")
        application = node(
            "frame",
            "MyApp",
            children=[node("label", "Random"), node("label", "")],
        )
        verdict = classify(exp, [application])
        self.assertEqual(verdict.verdict, "FAIL")
        self.assertNotEqual(verdict.prohibited_outcome, "name-empty")


class ExpectationsFileValidation(unittest.TestCase):
    """O1/B2: the loader must fail closed on a malformed or stale oracle
    before any live AT-SPI readback — never a FAIL, always a usage error
    that the caller (`run_check5`) turns into exit 2."""

    VALID_DIGEST = "deadbeef" * 8  # a plausible-looking 64-hex-char sha256

    @staticmethod
    def _fixture(fixture_id, name, alternative_forms=None, source_atoms=None):
        return {
            "fixture_id": fixture_id,
            "expected_name": name,
            "expected_name_hex": name.encode("utf-8").hex(),
            "expected_name_byte_len": len(name.encode("utf-8")),
            "accepted_roles": ["label", "static", "text", "paragraph"],
            "prohibited_roles": ["image", "canvas", "filler", "panel", "unknown"],
            # Defaults to one atom equal to the whole name (trivially
            # satisfies the join-equals-name invariant) — tests of other
            # fields don't need more than one segment.
            "source_atoms": source_atoms if source_atoms is not None else [name],
            "alternative_forms": alternative_forms or {},
        }

    def _valid_file(self):
        """A fully self-consistent, five-fixture file — every B2/O1/D1 check
        passes against this by construction. Each test below mutates
        exactly one thing away from it, so a raised error is attributable to
        the one defect under test rather than an incidental other one."""
        return {
            "contract": "spec/CONTRACT_EDITOR_T4_SPIKE.md pin 13",
            "recipe": "spikes/editor-toolkit/ROUND2_TEXT_RECIPE.md §8",
            "platform": PLATFORM,
            "source_fixtures_digest": self.VALID_DIGEST,
            "fixtures": [
                self._fixture("F-A", "Allegro affettuoso — al fine"),
                self._fixture("F-B", "Coro אבג", source_atoms=["Coro ", "אבג"]),
                self._fixture("F-C", "Coro ا", source_atoms=["Coro ", "ا"]),
                self._fixture(
                    "F-D", "Allegro אבג con brio", source_atoms=["Allegro ", "אבג", " con brio"]
                ),
                self._fixture("F-E", "Café"),
            ],
        }

    def _validate(self, file):
        validate_expectations_file(
            file, expected_platform=PLATFORM, expected_source_digest=self.VALID_DIGEST
        )

    # ---- baseline ----

    def test_a_fully_valid_file_is_accepted(self):
        self._validate(self._valid_file())  # must not raise

    # ---- platform ----

    def test_a_wrong_platform_is_refused(self):
        bad = self._valid_file()
        bad["platform"] = "aria"
        with self.assertRaises(ValueError) as ctx:
            self._validate(bad)
        msg = str(ctx.exception)
        self.assertIn("aria", msg)
        self.assertIn(PLATFORM, msg)

    # ---- source_fixtures_digest (B2) ----

    def test_a_stale_source_digest_is_refused(self):
        bad = self._valid_file()
        bad["source_fixtures_digest"] = "stale" + "0" * 60
        with self.assertRaises(ValueError) as ctx:
            self._validate(bad)
        msg = str(ctx.exception)
        self.assertIn("stale", msg)
        self.assertIn(self.VALID_DIGEST, msg)

    # ---- fixture id completeness/uniqueness ----

    def test_a_duplicate_fixture_id_is_refused(self):
        bad = self._valid_file()
        bad["fixtures"].append(self._fixture("F-A", "duplicate"))
        with self.assertRaises(ValueError) as ctx:
            self._validate(bad)
        self.assertIn("duplicate", str(ctx.exception).lower())
        self.assertIn("F-A", str(ctx.exception))

    def test_a_missing_fixture_is_refused(self):
        bad = self._valid_file()
        bad["fixtures"] = [f for f in bad["fixtures"] if f["fixture_id"] != "F-E"]
        with self.assertRaises(ValueError) as ctx:
            self._validate(bad)
        self.assertIn("F-E", str(ctx.exception))

    def test_an_extra_fixture_id_is_refused(self):
        bad = self._valid_file()
        bad["fixtures"].append(self._fixture("F-Z", "unexpected"))
        with self.assertRaises(ValueError) as ctx:
            self._validate(bad)
        self.assertIn("F-Z", str(ctx.exception))

    def test_the_expected_fixture_id_set_is_exactly_f_a_through_f_e(self):
        """Anchors `EXPECTED_FIXTURE_IDS` itself, independent of
        `validate_expectations_file` — if this constant silently gained or
        lost an id, the two tests above could pass against the wrong set."""
        self.assertEqual(EXPECTED_FIXTURE_IDS, frozenset({"F-A", "F-B", "F-C", "F-D", "F-E"}))

    # ---- expected_name / expected_name_hex / expected_name_byte_len self-consistency ----

    def test_a_wrong_hex_is_refused(self):
        bad = self._valid_file()
        bad["fixtures"][0]["expected_name_hex"] = "ff" * 10
        with self.assertRaises(ValueError) as ctx:
            self._validate(bad)
        msg = str(ctx.exception)
        self.assertIn("F-A", msg)
        self.assertIn("hex", msg)

    def test_an_uppercase_hex_is_refused(self):
        """§8.1 specifically requires *lowercase* hex — an otherwise-correct
        but uppercase rendering must still be refused, not accepted as
        "close enough"."""
        bad = self._valid_file()
        bad["fixtures"][0]["expected_name_hex"] = bad["fixtures"][0]["expected_name_hex"].upper()
        with self.assertRaises(ValueError) as ctx:
            self._validate(bad)
        self.assertIn("hex", str(ctx.exception))

    def test_a_wrong_byte_length_is_refused(self):
        bad = self._valid_file()
        bad["fixtures"][0]["expected_name_byte_len"] += 1
        with self.assertRaises(ValueError) as ctx:
            self._validate(bad)
        msg = str(ctx.exception)
        self.assertIn("F-A", msg)
        self.assertIn("byte_len", msg)

    def test_a_correct_hex_and_length_pair_is_not_refused(self):
        """Mutation guard: confirms the two tests above are checking the
        actual computed hex/length, not merely "is a string of digits" or
        some other weaker property."""
        self._validate(self._valid_file())  # must not raise

    # ---- O1: cross-outcome collision (unchanged, folded into this entry point) ----

    def test_a_colliding_file_is_refused_naming_the_fixture_and_both_outcomes(self):
        bad = self._valid_file()
        bad["fixtures"][2]["alternative_forms"] = {  # F-C
            "name-drops-unresolved-codepoints": ["Coro "],
            "name-is-shaped-glyphs": ["Coro "],
        }
        with self.assertRaises(ValueError) as ctx:
            self._validate(bad)
        msg = str(ctx.exception)
        self.assertIn("F-C", msg)
        self.assertIn("name-drops-unresolved-codepoints", msg)
        self.assertIn("name-is-shaped-glyphs", msg)

    def test_a_collision_between_list_entries_is_also_refused(self):
        """The collision can be between any entry in one outcome's list and
        any entry in another's, not just single-value outcomes."""
        bad = self._valid_file()
        bad["fixtures"][0]["alternative_forms"] = {  # F-A
            "name-normalized": ["alpha", "shared"],
            "name-is-shaped-glyphs": ["beta", "shared"],
        }
        with self.assertRaises(ValueError) as ctx:
            self._validate(bad)
        self.assertIn("shared", str(ctx.exception))

    def test_a_repeated_value_within_the_same_outcome_is_not_a_collision(self):
        """Mutation guard: if the collision check fired on *any* repeated
        value rather than specifically a *cross-outcome* one, this would
        wrongly raise — two entries in one outcome's own list happening to
        repeat is not the ambiguity O1 refuses."""
        fine = self._valid_file()
        fine["fixtures"][0]["alternative_forms"] = {"name-normalized": ["same", "same"]}
        self._validate(fine)  # must not raise

    def test_the_real_committed_file_is_valid(self):
        """Grounds the synthetic tests above in the actual generated
        artifact — the file `run_check5` will really load. Uses the file's
        own `platform`/`source_fixtures_digest` as the "expected" values
        (this is the one place a real digest isn't known statically), so
        this test is really only exercising the fixture-id/name-consistency/
        collision checks against real data, not the digest-mismatch check."""
        import json
        import os

        path = os.path.join(
            os.path.dirname(__file__), "..", "round2-a11y-oracle", "a11y_expectations.json"
        )
        if not os.path.exists(path):
            self.skipTest(f"{path} absent — run the round2-a11y-oracle generator first")
        with open(path, "r", encoding="utf-8") as f:
            real_file = json.load(f)
        validate_expectations_file(
            real_file,
            expected_platform=real_file.get("platform"),
            expected_source_digest=real_file.get("source_fixtures_digest"),
        )  # must not raise


if __name__ == "__main__":
    unittest.main()
