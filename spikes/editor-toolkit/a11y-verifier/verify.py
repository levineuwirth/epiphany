#!/usr/bin/env python3
"""AT-SPI2 accessibility verifier — Round 0 readback mode, and Round 2 check 5.

An AT-SPI client, independent of any candidate's own process, that walks the
live platform accessibility tree (via the AT-SPI2 registry over D-Bus). Two
modes, selected by which flags are given:

Round 0 mode — unchanged, byte-for-byte, from the version that produced
`round0-evidence/c1-egui-readback.txt` and `c2-vello-readback.txt`:

    verify.py --role "push button" --name "EpiphanyProbeButton" [--app-name SUBSTR] [--max-depth N] [--timeout SECONDS]

Looks for one exact (role, name) match anywhere under the desktop (optionally
restricted to apps whose name contains --app-name). Exit 0 "READBACK: PASS",
exit 1 "READBACK: FAIL", exit 2 "READBACK: NOT RUN" (bus unreachable).

Round 2 check 5 mode — `spikes/editor-toolkit/ROUND2_TEXT_RECIPE.md` §8, an
accessibility oracle packet 2B-A precommits (`round2-a11y-oracle`):

    verify.py --expectations round2-a11y-oracle/a11y_expectations.json --fixture F-A \
        --expect-source-digest <round2_textkit::output::expected_artifact_digest()> \
        --app-name SUBSTR [--timeout N] [--json PATH]

Scores one fixture's check 5 against the live tree under the candidate's
application (matched by --app-name, required in this mode). Exit 0 "CHECK5:
PASS", exit 1 "CHECK5: FAIL" (naming exactly one of
`round2_textkit::a11y::PROHIBITED_OUTCOMES`, or a role/composition-specific
diagnosis, when applicable), exit 2 "CHECK5: NOT RUN" — reserved *only* for
the AT-SPI bus itself being unreachable. A candidate that simply never built
an accessibility tree is `absent-from-tree`, which is a FAIL, not NOT RUN.

Uses gi.repository.Atspi, the official GObject-introspection binding for
AT-SPI2 (the same library backing Orca and Accerciser). Used in place of the
`atspi` Rust crate as an "equivalent AT-SPI client" (the contract's own
wording) — chosen because its API is stable, documented, and already
verified reachable on this machine, rather than reverse-engineering an
unfamiliar async zbus proxy API under this round's timebox. That substitution
is a named deviation, reported as such.
"""
import argparse
import json
import sys
import time
from dataclasses import dataclass, field
from typing import Dict, List, Optional, Tuple

# ---------------------------------------------------------------------------
# Round 0 mode — unmodified from the version that produced the committed
# round0-evidence transcripts. Do not change this function's behaviour.
# ---------------------------------------------------------------------------


def walk(node, role, name, app_name_substr, max_depth, path, found, all_seen):
    if node is None:
        return
    try:
        node_name = node.get_name()
    except Exception:
        node_name = "<error>"
    try:
        node_role = node.get_role_name()
    except Exception:
        node_role = "<error>"
    all_seen.append(" / ".join(path + [f"{node_role}:{node_name!r}"]))
    if node_role == role and node_name == name:
        found.append(list(path) + [f"{node_role}:{node_name!r}"])
        return
    if max_depth <= 0:
        return
    try:
        n = node.get_child_count()
    except Exception:
        return
    for i in range(n):
        try:
            child = node.get_child_at_index(i)
        except Exception:
            continue
        walk(
            child,
            role,
            name,
            app_name_substr,
            max_depth - 1,
            path + [f"{node_role}:{node_name!r}"],
            found,
            all_seen,
        )


def run_round0(args, Atspi):
    try:
        Atspi.init()
    except Exception as exc:
        print(f"READBACK: NOT RUN — Atspi.init() failed: {exc}")
        sys.exit(2)

    deadline = time.monotonic() + args.timeout
    last_seen = []
    attempt = 0
    while time.monotonic() < deadline:
        attempt += 1
        try:
            desktop = Atspi.get_desktop(0)
        except Exception as exc:
            print(f"READBACK: NOT RUN — Atspi.get_desktop(0) failed: {exc}")
            sys.exit(2)
        if desktop is None:
            print("READBACK: NOT RUN — Atspi.get_desktop(0) returned None (no AT-SPI registry?)")
            sys.exit(2)

        found = []
        all_seen = []
        try:
            n_apps = desktop.get_child_count()
        except Exception as exc:
            print(f"READBACK: NOT RUN — desktop.get_child_count() failed: {exc}")
            sys.exit(2)

        for i in range(n_apps):
            try:
                app = desktop.get_child_at_index(i)
            except Exception:
                continue
            if app is None:
                continue
            try:
                app_name = app.get_name()
            except Exception:
                app_name = "<error>"
            if args.app_name and args.app_name not in app_name:
                continue
            walk(
                app,
                args.role,
                args.name,
                args.app_name,
                args.max_depth,
                [f"desktop"],
                found,
                all_seen,
            )

        last_seen = all_seen
        if found:
            print("READBACK: PASS")
            print(f"attempt: {attempt}, elapsed: {args.timeout - (deadline - time.monotonic()):.2f}s")
            print("path: " + " / ".join(found[0]))
            print(f"apps enumerated: {n_apps}")
            print("full tree (role:name) seen during the matching walk:")
            for line in all_seen:
                print("  " + line)
            sys.exit(0)

        time.sleep(args.poll_interval)

    print("READBACK: FAIL")
    print(f"no node with role={args.role!r} name={args.name!r} found within {args.timeout}s ({attempt} attempts)")
    print("nodes actually seen (role:name), last attempt:")
    if not last_seen:
        print("  <none — desktop had 0 matching/enumerable apps>")
    for line in last_seen:
        print("  " + line)
    sys.exit(1)


# ---------------------------------------------------------------------------
# Round 2 check 5 mode.
# ---------------------------------------------------------------------------

# A verifier-specific diagnostic name for the §8.1 composition trap — the
# concatenation matches `visual_order_name`, not `expected_name`. This is
# deliberately *not* one of `round2_textkit::a11y::PROHIBITED_OUTCOMES`: it is
# a structural composition failure (assembled in the wrong order), not one of
# the five name-transformation outcomes §8.3 pins. Naming it distinctly is
# the whole point of the requirement: "the report must name it rather than
# emit a generic mismatch."
VISUAL_ORDER_TRAP = "composed-in-visual-order"

# The platform row this verifier scores against — this machine's live AT
# client is AT-SPI2 (recipe §8.2, round0-evidence's precedent), matching
# `round2_a11y_oracle::PLATFORM`. Used by `validate_expectations_file` (B2)
# to refuse an expectations file generated for a different platform, rather
# than silently scoring against the wrong role vocabulary.
PLATFORM = "at-spi2"

# The exact five fixture ids the recipe names (ROUND2_TEXT_RECIPE.md §2),
# restated here — not read back out of the file being validated — the same
# discipline `round2_textkit::output::FixtureFile::validate`'s
# `EXPECTED_FIXTURES` uses, so a file missing one or carrying an extra id is
# caught against a literal, not against its own other contents.
EXPECTED_FIXTURE_IDS = frozenset({"F-A", "F-B", "F-C", "F-D", "F-E"})


def validate_expectations_file(
    expectations_file: dict, *, expected_platform: str, expected_source_digest: str
) -> None:
    """B2/O1: fail closed on a malformed or stale oracle **before** any live
    AT-SPI readback. Check 5 is disqualifying, so a defect in the oracle
    artifact itself must never be silently absorbed into a candidate's
    verdict — every check below raises `ValueError` (which the caller turns
    into a usage error, exit 2, never a FAIL: a malformed oracle is not a
    candidate defect) naming exactly what disagreed.

    - `platform` must equal `expected_platform` — scoring F-A's at-spi2 role
      vocabulary against a file generated for a different platform would
      silently check the wrong roles.
    - `source_fixtures_digest` must equal `expected_source_digest` — the
      caller passes `round2_textkit::output::expected_artifact_digest()`
      (via `--expect-source-digest`), so an oracle generated against a
      *different* `fixtures.json` (stale, or regenerated on a machine with
      different fonts — recipe §1) cannot score a candidate under the
      pretense of being current.
    - The fixture id set is exactly `EXPECTED_FIXTURE_IDS`: no duplicates, none
      missing, none extra.
    - Per fixture, `expected_name` / `expected_name_hex` / `expected_name_byte_len`
      are mutually consistent — recipe §8.1 carries the name three ways
      specifically so a divergence between them is detectable; this is what
      detects it. (lowercase hex, per §8.1's own "lowercase hex" wording.)
    - D1: `source_atoms` is a list of strings whose concatenation, in order,
      equals `expected_name` — the same partition property
      `round2_a11y_oracle::source_atoms`'s own doc comment claims and tests
      on the generation side; this is the verifier-side half of that same
      check, so a hand-edited or differently-generated file cannot silently
      carry atoms that no longer add up to the name they are supposed to be
      components of.
    - O1: no two different outcome names in one fixture's `alternative_forms`
      produce the same string (unchanged from the earlier fix, folded into
      this same fail-closed entry point).

    Does not touch AT-SPI or any live state, so it is testable without a bus,
    the same as `classify`.
    """
    actual_platform = expectations_file.get("platform")
    if actual_platform != expected_platform:
        raise ValueError(
            f"platform is {actual_platform!r}, expected {expected_platform!r} — this oracle was "
            "not generated for the platform being scored"
        )

    actual_digest = expectations_file.get("source_fixtures_digest")
    if actual_digest != expected_source_digest:
        raise ValueError(
            f"source_fixtures_digest is {actual_digest!r}, expected {expected_source_digest!r} "
            "(round2_textkit::output::expected_artifact_digest()) — this oracle may have been "
            "generated against a different fixtures.json and must not score a candidate"
        )

    fixtures = expectations_file.get("fixtures", [])
    ids = [fx.get("fixture_id") for fx in fixtures]
    if len(ids) != len(set(ids)):
        duplicates = sorted({i for i in ids if ids.count(i) > 1})
        raise ValueError(f"duplicate fixture_id(s) in expectations file: {duplicates}")
    id_set = set(ids)
    missing = sorted(EXPECTED_FIXTURE_IDS - id_set)
    extra = sorted(id_set - EXPECTED_FIXTURE_IDS)
    if missing or extra:
        raise ValueError(
            f"fixture id set is {sorted(id_set)}, expected exactly {sorted(EXPECTED_FIXTURE_IDS)} "
            f"(missing: {missing}, extra: {extra})"
        )

    for fx in fixtures:
        fixture_id = fx.get("fixture_id", "<unknown>")

        name = fx.get("expected_name")
        name_hex = fx.get("expected_name_hex")
        name_byte_len = fx.get("expected_name_byte_len")
        if not isinstance(name, str):
            raise ValueError(f"{fixture_id!r}: expected_name is not a string: {name!r}")
        actual_name_bytes = name.encode("utf-8")
        actual_hex = actual_name_bytes.hex()  # Python's .hex() is always lowercase
        if name_hex != actual_hex:
            raise ValueError(
                f"{fixture_id!r}: expected_name_hex is {name_hex!r}, but the lowercase hex of "
                f"expected_name's UTF-8 bytes is {actual_hex!r} — the name and its hex have "
                "diverged"
            )
        if name_byte_len != len(actual_name_bytes):
            raise ValueError(
                f"{fixture_id!r}: expected_name_byte_len is {name_byte_len!r}, but "
                f"expected_name's UTF-8 byte length is {len(actual_name_bytes)}"
            )

        atoms = fx.get("source_atoms")
        if not isinstance(atoms, list) or not all(isinstance(a, str) for a in atoms):
            raise ValueError(f"{fixture_id!r}: source_atoms is not a list of strings: {atoms!r}")
        joined_atoms = "".join(atoms)
        if joined_atoms != name:
            raise ValueError(
                f"{fixture_id!r}: source_atoms {atoms!r} concatenate to {joined_atoms!r}, which "
                f"does not equal expected_name {name!r} — the atoms no longer partition the name "
                "they are supposed to be components of"
            )

        forms_by_outcome: Dict[str, List[str]] = fx.get("alternative_forms", {}) or {}
        owner_of: Dict[str, str] = {}
        for outcome, forms in forms_by_outcome.items():
            for form in forms:
                existing = owner_of.get(form)
                if existing is not None and existing != outcome:
                    raise ValueError(
                        f"{fixture_id!r}: alternative forms {existing!r} and {outcome!r} both "
                        f"produce {form!r} — an oracle that returns two different "
                        "classifications for the same observed string must fail closed, not "
                        "let iteration order pick one"
                    )
                owner_of[form] = outcome


@dataclass
class Verdict:
    """One check-5 scoring outcome. `verdict` is always exactly one of
    "PASS" / "FAIL" (`prohibited_outcome` distinguishes NOT RUN, which is
    handled by the caller before a `Verdict` is ever constructed — NOT RUN is
    reserved for the AT-SPI bus itself being unreachable, never for a
    classification the tree walk produced)."""

    verdict: str
    reason: str
    observed_role: Optional[str] = None
    observed_name: Optional[str] = None
    prohibited_outcome: Optional[str] = None


@dataclass
class ObservedNode:
    """One node of a live AT-SPI subtree, as walked by `walk_for_check5` —
    role, name, and children, preserving the structure `classify` needs to
    score §8.1 composition per-subtree (B1). Deliberately holds nothing else
    (no live AT-SPI object reference): once built, this is inert data, which
    is what lets `classify` stay pure and bus-free."""

    role: str
    name: str
    children: List["ObservedNode"] = field(default_factory=list)


def _iter_nodes(node: ObservedNode):
    """Every node in `node`'s subtree, `node` itself included, pre-order."""
    yield node
    for child in node.children:
        yield from _iter_nodes(child)


def _iter_forest(roots: List[ObservedNode]):
    """Every node in every tree in `roots`, pre-order, roots first."""
    for root in roots:
        yield from _iter_nodes(root)


def _flatten_candidates(
    roots: List[ObservedNode], accepted: set, prohibited: set
) -> List[Tuple[str, str]]:
    """Every `(role, name)` pair, for every node anywhere in the forest whose
    role is a text-candidate (`accepted | prohibited`), in tree order.

    Used for exactly one thing now: `classify`'s final `name-empty` vs.
    `absent-from-tree` decision (user ruling), reached only after every
    source-bearing scan (which considers *every* role, not just
    `accepted | prohibited`) has found nothing. `name-empty` is specifically
    about accepted/prohibited-role candidates existing with no name, so it
    is the one remaining check that legitimately wants this narrower,
    role-filtered list rather than the whole forest.
    """
    return [
        (n.role, n.name) for n in _iter_forest(roots) if n.role in accepted or n.role in prohibited
    ]


def _all_descendants(root: ObservedNode) -> List[Tuple[str, str]]:
    """Every `(role, name)` pair for **every** descendant of `root` **with a
    non-empty name**, regardless of role — `root` itself excluded, since a
    node's own name matching `expected_name` (or an alternative/visual-order
    form) is the separate single-node case (§8.1's first alternative; this
    is its second), in tree order.

    Deliberately **not** filtered by role before the caller concatenates: a
    non-accepted-role contributor's name is still part of what the
    subtree's composition actually says, and dropping it before summing
    would let a subtree "pass" by silently ignoring a contributor it
    doesn't like — precisely the wrong fix for B1. The caller concatenates
    first, checks role-acceptability only once the concatenation is already
    confirmed to equal `expected_name` (or a precommitted alternative/
    visual-order form).

    **Empty-named nodes are excluded entirely, not merely ignored when
    picking whom to blame.** An empty name contributes zero bytes to the
    concatenation — including or excluding it never changes `subtree_concat`
    — so the only thing including it can do is let a purely structural
    wrapper (a `frame` or `panel` around the real contributors, exposing no
    name of its own) be *named* as the offending contributor merely because
    it happens to sort first in tree order, hiding the actual, non-empty,
    possibly prohibited-role contributor that is the real §8.2 violation.
    Excluding it here, at the source, fixes this the same way regardless of
    which subtree in `classify`'s scan happens to be tried (and matched)
    first — relying on the wrapper's own name to corrupt a *different*
    subtree's concatenation would only fix the cases where that subtree
    happened to be visited later.

    Used by `classify`'s composition scan for **every** subtree, regardless
    of which roles (if any) appear elsewhere in the tree — an earlier
    version of this function only admitted unlisted-role contributors when
    *no* accepted-or-prohibited-role node existed anywhere in the tree,
    which is exactly the gating the "absent-from-tree vs. name-empty" fix
    removed: a real application's window `label` must not prevent the run's
    actual text, exposed under an unlisted role elsewhere in the same tree,
    from being found.
    """
    out: List[Tuple[str, str]] = []
    for child in root.children:
        for n in _iter_nodes(child):
            if n.name != "":
                out.append((n.role, n.name))
    return out


def _is_source_bearing_fragment(name: str, targets) -> bool:
    """**One of two additive paths** (D1) `classify`'s fragment scan uses to
    decide "the run's text is present, even if not composed correctly"
    (user ruling, following C3) — this is the general, heuristic,
    coincidence-guarded substring rule; `source_atoms` exact matching (see
    `classify`'s fragment scan) is the other, precommitted, no-length-floor
    path. To distinguish real (if misordered or incomplete) evidence of the
    run from an unrelated node's text that happens to share a coincidental
    substring, `name` counts as a source-bearing fragment of one of
    `targets` (`expected_name`, or a precommitted alternative/visual-order
    form) only if it is:

    - non-empty and not whitespace-only (`name.strip()` is non-empty) — a
      bare space is not evidence of anything, even though a space is
      technically a substring of e.g. `"Coro "`;
    - **at least two characters** after stripping — a single character is
      not distinguishable from coincidence: almost any two unrelated
      strings of ordinary language share *some* one character (a window
      title and `"Coro אבג"` both very plausibly contain the letter `"o"`);
    - a literal substring of at least one target, compared **as given** —
      never normalized, and the containment test itself uses the raw
      (unstripped) `name`, so incidental surrounding whitespace in `name`
      that isn't present in the target correctly fails to match; only the
      length/whitespace *gate* above is computed on the stripped form.

    **Stated limit, not hidden — this rule deliberately under-detects, and
    is deliberately never loosened to cover it.** A genuine run fragment
    shorter than two characters — F-C's unresolved segment `ا` is exactly
    this case, a single character — is never caught by *this* function, on
    purpose: loosening the floor to catch it would risk exactly what
    `SourceBearingFragmentGuards`' guard tests exist to catch — an
    application's ordinary window title coincidentally sharing a short
    substring with `expected_name` and permanently disabling
    `absent-from-tree` for that fixture, "which is a worse failure than the
    one being fixed" (the ruling's own words). F-C's single-character
    segment is instead caught by the *other* path — an exact match against
    a precommitted `source_atoms` entry, which needs no length floor at all
    because it is a comparison against precommitted data, not a heuristic
    guess from length alone. The two paths are independent; this function's
    own contract does not change.
    """
    stripped = name.strip()
    if len(stripped) < 2:
        return False
    return any(name in target for target in targets)


def classify(expectation: dict, roots: List[ObservedNode]) -> Verdict:
    """The whole of check 5's scoring logic, and nothing else.

    `expectation` is one fixture's entry from `a11y_expectations.json`
    (`round2-a11y-oracle`) — a plain dict with `expected_name`,
    `accepted_roles`, `prohibited_roles`, `alternative_forms` (an outcome
    name mapped to a **list** of precommitted forms — O2: one outcome can
    have more than one plausible rendering, e.g. `name-is-shaped-glyphs`
    carries both a cluster-collapse form and a ligature presentation-form
    substitution for F-A; matched if the observed name equals *any* entry),
    and (optionally) `visual_order_name`. The caller must have already run
    this file through `validate_expectations_file` (O1/B2) — `classify`
    itself does not re-check the oracle's own integrity, since a malformed
    oracle is exactly what validation exists to refuse before this function
    ever runs.

    `roots` is the forest of `ObservedNode` trees the live tree walk found
    under the candidate's application (usually one tree, the matched app's
    own node) — this function does not touch AT-SPI, D-Bus, or any live
    state, which is what makes it testable without a bus.

    **Shape (user ruling): source-bearing detection runs first, in full,
    across every role, before any absence or empty-name determination —
    never the other way around.** An earlier version of this function only
    looked for the run's text under unlisted roles when *no*
    accepted-or-prohibited-role node existed anywhere in the tree at all.
    That gate was wrong: a real application always has *some* accepted-role
    node (a window title `label`, at minimum), so the run's actual text,
    exposed under an unlisted role *alongside* that unrelated label, was
    never even looked for — the tree fell straight into the ordinary
    (non-source-bearing) scoring path and reported whatever that path says
    for "some accepted-role text exists, none of it matches," which used to
    be a generic mismatch and is now (see below) `absent-from-tree`. So
    every check in this section runs over the **whole forest, every role,
    unconditionally** — never against just the first match, and never
    gated on whether some *other*, unrelated node happens to carry an
    accepted or prohibited role.

    **PRECEDENCE (pinned, C1) — exact-name matches.** §8.1's rule — "the
    run's own accessible name ... must equal the source string" — is
    evaluated over **every** node in the forest, regardless of role, not the
    first one found. If *any* node with an **accepted** role carries
    `expected_name` byte-for-byte, the verdict is PASS, regardless of where
    in the tree that node sits or whether some *other* node (prohibited- or
    unlisted-role) also happens to carry it. Failing that, a **prohibited**-
    role match is named preferentially over an **unlisted**-role one (more
    specific, per §8.2's own vocabulary); failing that, an unlisted-role
    match is named. This is a pinned rule, not an implementation shortcut: a
    tree that lists a `canvas` node before the real `text` node is exactly
    the same candidate as one that lists them in the other order, and must
    score the same way. Do not "simplify" this back to returning on the
    first exact-name match — that reintroduces order-dependence on a
    disqualifying check.

    **B1/C2: composition and its alternative-form/visual-order diagnoses are
    all scored per subtree, never against a whole-application
    concatenation, and admit every role as a contributor.** §8.1's second
    alternative — "the names of its text descendants concatenated in
    logical order" — is a statement about *one run's* subtree, and nothing
    in §8.1 restricts which roles may compose it (an unlisted role composing
    correctly is still wrong — see the fragment/role check below — but that
    is a role failure to report, not a reason to exclude the node from the
    concatenation in the first place). This function tries every node in the
    forest as a candidate "this is the run" subtree root in turn, and for
    each one:

    - if that subtree's own descendants (any role) concatenate
      byte-exactly to `expected_name` **and** every one of those
      descendants has an accepted role, PASS;
    - if they concatenate to `expected_name` but include a non-accepted-role
      contributor, that is a FAIL naming that contributor specifically — an
      otherwise-correct composition failed by one contributor's role,
      **never** silently dropped from consideration or averaged away by
      unrelated nodes elsewhere in the tree (B1's original bug: a stray
      `canvas` node absorbed into a whole-application PASS; C2's bug on the
      diagnosis side: a stray `label` node corrupting an F-D-style
      visual-order composition into a generic mismatch instead of naming
      `composed-in-visual-order`);
    - if instead they concatenate to one of a `PROHIBITED_OUTCOMES`
      alternative form, or to `visual_order_name`, that subtree's diagnosis
      is recorded (not returned immediately — a PASS found in a *different*
      subtree still wins, since a candidate that got it right anywhere in a
      legitimate run subtree has satisfied §8.1).

    A single node's own name is also checked against every alternative form
    and `visual_order_name` (not only `expected_name`), regardless of role —
    that has no subtree/aggregation ambiguity (one node's own name is
    unambiguous regardless of tree position), so it stays a simple
    whole-forest scan.

    **PRECEDENCE (pinned, D2) — a byte-exact PASS outranks every
    alternative-form or visual-order diagnosis, per-node or per-subtree.**
    Both PASS checks above (exact-name, and composition) already scan the
    *entire* forest before either can return a PASS, so evaluating them
    first and in full is what makes this safe: nothing is skipped to get to
    the diagnosis checks below them. Concretely, the single-node and
    subtree-level alternative-form/visual-order checks run **only after**
    both PASS checks have been exhausted with nothing found — never
    interleaved with them. This is why F-C's legitimate two-node split
    (`text:"Coro "` + `text:"ا"`, both accepted — exactly the "one text node
    per direction run" composition §8.1 permits) PASSes even though
    `"Coro "` alone happens to equal F-C's own precommitted
    `name-drops-unresolved-codepoints` form: the composition check finds the
    byte-exact two-node PASS first. Do not "simplify" this by moving an
    alternative-form check earlier for convenience — doing so previously
    turned a legitimate F-C composition into a false FAIL naming a
    `PROHIBITED_OUTCOMES` name that did not apply.

    **C3: fragments of the run's text present anywhere, under any role, even
    out of the logical order §8.1 requires, are still evidence against
    absence.** Failing an exact single-node or composition match above, any
    node meeting the narrow `_is_source_bearing_fragment` definition (see
    its own doc comment for the rule and its stated limits) is still
    evidence the run's text is present, however it is arranged — this is
    the case an exact-match/composition scan alone cannot see: text that is
    genuinely present but misordered or incomplete.

    **`absent-from-tree` vs. `name-empty` (user ruling, pinned) — decided
    only after every check above has found nothing.** The distinction being
    preserved: `name-empty` means an attempted **static-text exposure**
    without a name (§8.3: "absence wearing a role"); `absent-from-tree`
    covers a **drawing-only** tree or an **unrelated** one. Concretely:

    - `name-empty` fires **only** when both hold: at least one
      **accepted**-role candidate node exists somewhere in the tree, *and*
      every accepted-or-prohibited-role candidate's name is empty. A lone
      empty **prohibited**-role node (a canvas that drew nothing and
      exposed nothing) is *not* "wearing a role" in §8.3's sense — it is
      the draw-and-stop case §8.3 calls "the one this check will most
      likely actually catch," and it is `absent-from-tree`.
    - every other case that reaches this point — a genuinely empty tree, a
      drawing-only tree, or a tree whose only text (under any role) bears no
      relation to the run at all — is `absent-from-tree`.

    The required regression lock for this exact distinction lives in
    `AbsentFromTreeVsNameEmpty` (`test_verify.py`):

        unrelated UI text only              -> absent-from-tree
        empty prohibited canvas only        -> absent-from-tree
        empty accepted text/label node      -> name-empty
        misordered source fragments         -> composition/role failure, never absence

    **Contributor order stays semantically significant everywhere in this
    function** — only **non-contributor** permutations (an unrelated
    sibling moving around the tree) are required to be verdict-invariant.
    This function never "fixes" composition into an order-insensitive
    match; that would defeat the entire point of the F-D visual-order trap
    (§8.1).

    Comparisons are always on the Python `str` (which is Unicode
    codepoints), never bytes directly, but every string compared here is
    already the exact source string on the Rust side (`str == str` is
    codepoint-exact, which for valid UTF-8 is byte-exact) — the caller is
    responsible for hex-encoding whatever `observed_name` this returns if a
    byte-level report is needed (see `run_check5`).
    """
    expected_name = expectation["expected_name"]
    accepted = set(expectation["accepted_roles"])
    prohibited = set(expectation["prohibited_roles"])
    alt_forms: Dict[str, List[str]] = expectation.get("alternative_forms", {}) or {}
    visual_order_name = expectation.get("visual_order_name")
    # D1: precommitted per-segment source atoms (`round2-a11y-oracle`'s
    # `source_atoms`), e.g. F-C's `["Coro ", "ا"]`. A node name exactly
    # matching one is source-bearing regardless of length — this is what
    # catches F-C's single-character unresolved segment `ا`, which the
    # length-2 `_is_source_bearing_fragment` substring rule cannot (and must
    # not be loosened to) catch on its own.
    source_atoms = set(expectation.get("source_atoms", []) or [])

    interesting_names = {expected_name}
    for forms in alt_forms.values():
        interesting_names.update(forms)
    if visual_order_name is not None:
        interesting_names.add(visual_order_name)

    # Every node in the forest, any role — the source-bearing scans below
    # are unconditional on role, per the user ruling: gating them on whether
    # some *other*, unrelated node happens to carry an accepted/prohibited
    # role is exactly the bug being fixed.
    all_nodes: List[Tuple[str, str]] = [(n.role, n.name) for n in _iter_forest(roots)]

    # 1. C1: exact-name matches, evaluated over the *entire* forest, every
    #    role, before deciding anything — never the first match found, and
    #    never gated on some other node's role.
    exact_matches = [(role, name) for role, name in all_nodes if name == expected_name]
    if exact_matches:
        accepted_matches = [rn for rn in exact_matches if rn[0] in accepted]
        if accepted_matches:
            role, name = accepted_matches[0]
            return Verdict(
                "PASS",
                f"a node with an accepted role ({role!r}) carries the accessible name "
                "byte-for-byte",
                observed_role=role,
                observed_name=name,
            )
        prohibited_matches = [rn for rn in exact_matches if rn[0] in prohibited]
        if prohibited_matches:
            role, name = prohibited_matches[0]
            return Verdict(
                "FAIL",
                f"a node's name matches expected_name byte-for-byte, but its role {role!r} "
                "is in the at-spi2 prohibited set (no accepted-role node also carries it)",
                observed_role=role,
                observed_name=name,
            )
        # Every remaining match's role is in neither accepted nor prohibited.
        role, name = exact_matches[0]
        return Verdict(
            "FAIL",
            f"a node's name matches expected_name byte-for-byte, but its role {role!r} is "
            "neither accepted nor prohibited for at-spi2 (no accepted- or prohibited-role node "
            "also carries it)",
            observed_role=role,
            observed_name=name,
        )

    # 2. B1/C2: composition and its alternative-form/visual-order diagnoses,
    #    all scored per subtree in one pass, every role admitted as a
    #    contributor. Try every node in the forest as a candidate run-subtree
    #    root; every check below is decided by that node's own descendants
    #    alone, never by nodes outside it.
    first_bad_composition: Optional[Verdict] = None
    first_alt_form_fail: Optional[Verdict] = None
    first_visual_order_fail: Optional[Verdict] = None
    for candidate_root in _iter_forest(roots):
        contributors = _all_descendants(candidate_root)
        if not contributors:
            continue
        subtree_concat = "".join(name for _, name in contributors)

        if subtree_concat == expected_name:
            bad = [(role, name) for role, name in contributors if role not in accepted]
            if not bad:
                return Verdict(
                    "PASS",
                    "the descendants of one run subtree concatenate to expected_name "
                    "byte-for-byte, and every contributor's role is accepted",
                    observed_name=subtree_concat,
                )
            if first_bad_composition is None:
                # Prefer naming a prohibited-role contributor over a merely
                # unlisted one: prohibited is the specific, named §8.2
                # divergence, and the report exists to say that, not the
                # weaker "nobody listed this role" case — pick the first
                # prohibited-role entry if any exists, else fall back to the
                # first non-accepted entry (necessarily unlisted-role, since
                # `bad` excludes accepted roles by construction).
                prohibited_bad = [rn for rn in bad if rn[0] in prohibited]
                bad_role, _bad_name = prohibited_bad[0] if prohibited_bad else bad[0]
                classification = (
                    "prohibited" if bad_role in prohibited else "neither accepted nor prohibited"
                )
                other_count = len(bad) - 1
                mention_others = (
                    f" ({other_count} other non-accepted contributor(s) also present)"
                    if other_count > 0
                    else ""
                )
                first_bad_composition = Verdict(
                    "FAIL",
                    "a run subtree's descendants concatenate to expected_name byte-for-byte, "
                    f"but contributor role {bad_role!r} is {classification} for at-spi2{mention_others} "
                    "— an otherwise-correct composition, failed by this contributor's role",
                    observed_role=bad_role,
                    observed_name=subtree_concat,
                )
            continue

        if first_alt_form_fail is None:
            for outcome, forms in alt_forms.items():
                if subtree_concat in forms:
                    first_alt_form_fail = Verdict(
                        "FAIL",
                        "one run subtree's concatenated contributors match a precommitted "
                        f"{outcome!r} alternative form byte-for-byte",
                        observed_name=subtree_concat,
                        prohibited_outcome=outcome,
                    )
                    break

        if (
            first_visual_order_fail is None
            and visual_order_name is not None
            and subtree_concat == visual_order_name
        ):
            first_visual_order_fail = Verdict(
                "FAIL",
                "one run subtree's concatenated contributors match visual_order_name, not "
                "expected_name — the tree was assembled by walking the visual runs left to "
                "right instead of logical order",
                observed_name=subtree_concat,
                prohibited_outcome=VISUAL_ORDER_TRAP,
            )

    if first_bad_composition is not None:
        return first_bad_composition

    # D2 (user ruling): a byte-exact PASS — single-node (step 1, above) or
    # subtree composition (step 2, above) — outranks every alternative-form
    # or visual-order diagnosis, per-node or per-subtree. Both PASS checks
    # already scan the *entire* forest before this point is ever reached, so
    # by construction nothing above this line has skipped a legitimate PASS
    # to get here. Only now, with every PASS opportunity exhausted, do the
    # alternative-form/visual-order diagnoses get a turn — starting with a
    # single node's own name (no subtree ambiguity: one node's own name is
    # unambiguous regardless of position or role, so this stays a flat,
    # whole-forest scan), then the subtree-level matches the composition
    # loop above already recorded.
    #
    # This ordering is why F-C's legitimate two-node split
    # (`text:"Coro "` + `text:"ا"`, both accepted) now PASSes even though
    # `"Coro "` alone is also F-C's precommitted `name-drops-unresolved-
    # codepoints` form: the composition loop above finds the byte-exact PASS
    # across both nodes and returns before this per-node check ever runs. A
    # single `text:"Coro "` node with **no** second node still reaches this
    # check (no composition to find), so the outcome stays named exactly as
    # before — see `FCTwoSegmentComposition`'s regression group
    # (`test_verify.py`) for both halves of that guarantee.
    for role, name in all_nodes:
        for outcome, forms in alt_forms.items():
            if name in forms:
                return Verdict(
                    "FAIL",
                    f"a node's name matches a precommitted {outcome!r} alternative form "
                    "byte-for-byte",
                    observed_role=role,
                    observed_name=name,
                    prohibited_outcome=outcome,
                )
        if visual_order_name is not None and name == visual_order_name:
            return Verdict(
                "FAIL",
                "a node's name matches visual_order_name, not expected_name — the tree was "
                "assembled by walking the visual runs left to right instead of logical order",
                observed_role=role,
                observed_name=name,
                prohibited_outcome=VISUAL_ORDER_TRAP,
            )
    if first_alt_form_fail is not None:
        return first_alt_form_fail
    if first_visual_order_fail is not None:
        return first_visual_order_fail

    # 3. C3/D1: fragments of the run's text present anywhere, any role, even
    #    when they do not compose to any target string in the required
    #    logical order — the case an exact-match/composition scan alone
    #    cannot see: text that is genuinely present but misordered or
    #    incomplete. Whole-forest, not subtree-scoped: the safety valve here
    #    is the narrow fragment definition itself
    #    (`_is_source_bearing_fragment`), not tree structure — the policy is
    #    "any fragment anywhere is evidence against absence," which a
    #    subtree restriction would contradict.
    #
    #    D1: a node counts as source-bearing via **either** of two additive
    #    paths — `_is_source_bearing_fragment`'s length-2-or-more substring
    #    rule, **or** an exact match against a precommitted `source_atoms`
    #    entry, regardless of length. The atom path is what catches F-C's
    #    unresolved segment `ا`: a single character, which the substring
    #    rule's coincidence guard correctly refuses (an unrelated stray "o"
    #    must never rescue a tree from absence) but which is nonetheless a
    #    real, precommitted, exact source component §8.3 requires to appear
    #    in the name. The two paths are independent and neither replaces the
    #    other — F-A (a single-segment run) has no atom shorter than its
    #    whole `expected_name`, so it depends entirely on the substring path,
    #    same as before D1.
    fragments = [
        (role, name)
        for role, name in all_nodes
        if _is_source_bearing_fragment(name, interesting_names) or name in source_atoms
    ]
    if fragments:
        roles = sorted({role for role, _ in fragments})
        fragment_concat = "".join(name for _, name in fragments)
        return Verdict(
            "FAIL",
            f"{len(fragments)} fragment(s) of the run's text are present under role(s) {roles}, "
            "but do not compose to expected_name or a precommitted form in the required logical "
            "order — a role/composition failure, not absent-from-tree",
            observed_role=roles[0] if len(roles) == 1 else None,
            observed_name=fragment_concat,
        )

    # 4. Nothing above found any source-bearing evidence anywhere, under any
    #    role, in any shape. Only one distinction remains (user ruling,
    #    pinned in the docstring above): `name-empty` requires an attempted
    #    *static-text* exposure — at least one accepted-role candidate node
    #    — with every accepted-or-prohibited-role candidate's name empty.
    #    Every other no-source-bearing case, including a lone empty
    #    prohibited-role node (draw-and-stop, §8.3's own headline case) and
    #    unrelated text under any role, is `absent-from-tree`.
    flat_candidates = _flatten_candidates(roots, accepted, prohibited)
    has_accepted_candidate = any(role in accepted for role, _ in flat_candidates)
    if (
        flat_candidates
        and has_accepted_candidate
        and all(name == "" for _, name in flat_candidates)
    ):
        return Verdict(
            "FAIL",
            "an accepted-role candidate node is present, but every accepted- or prohibited-role "
            "candidate's accessible name is empty — an attempted static-text exposure with no "
            "name",
            observed_name="",
            prohibited_outcome="name-empty",
        )

    return Verdict(
        "FAIL",
        "no accessible-text-candidate node (accepted or prohibited role) found under the "
        "candidate's application, on any single node, composed across any subtree, or as a "
        "source-bearing fragment, under any role",
        prohibited_outcome="absent-from-tree",
    )


def walk_for_check5(node, path, all_seen, max_depth) -> Optional[ObservedNode]:
    """Recursively mirrors the live AT-SPI subtree under `node` into an
    `ObservedNode` tree, and records every node's `role:name` into
    `all_seen` for the human/JSON "full tree" report — the same diagnostic
    output this produced before B1, alongside a tree instead of a flat list.

    Unlike the pre-B1 version, this does **not** decide which nodes are
    text-candidates — that decision now happens in `classify`, scoped per
    subtree (B1): filtering roles *while* flattening the walk into a list is
    exactly what threw away the subtree structure composition scoring needs.
    """
    if node is None:
        return None
    try:
        name = node.get_name()
    except Exception:
        name = "<error>"
    try:
        role = node.get_role_name()
    except Exception:
        role = "<error>"
    all_seen.append(" / ".join(path + [f"{role}:{name!r}"]))
    observed = ObservedNode(role=role, name=name)
    if max_depth <= 0:
        return observed
    try:
        n = node.get_child_count()
    except Exception:
        return observed
    for i in range(n):
        try:
            child = node.get_child_at_index(i)
        except Exception:
            continue
        child_observed = walk_for_check5(
            child, path + [f"{role}:{name!r}"], all_seen, max_depth - 1
        )
        if child_observed is not None:
            observed.children.append(child_observed)
    return observed


def hex_lower(s: Optional[str]) -> Optional[str]:
    if s is None:
        return None
    return s.encode("utf-8").hex()


def run_check5(args, Atspi):
    try:
        with open(args.expectations, "r", encoding="utf-8") as f:
            expectations_file = json.load(f)
    except Exception as exc:
        print(f"CHECK5: usage error — could not read/parse {args.expectations!r}: {exc}")
        sys.exit(2)

    try:
        validate_expectations_file(
            expectations_file,
            expected_platform=PLATFORM,
            expected_source_digest=args.expect_source_digest,
        )
    except ValueError as exc:
        print(f"CHECK5: usage error — {args.expectations!r} failed validation: {exc}")
        sys.exit(2)

    expectation = next(
        (f for f in expectations_file.get("fixtures", []) if f.get("fixture_id") == args.fixture),
        None,
    )
    if expectation is None:
        print(
            f"CHECK5: usage error — {args.fixture!r} is not a fixture in {args.expectations!r} "
            f"(has: {[f.get('fixture_id') for f in expectations_file.get('fixtures', [])]})"
        )
        sys.exit(2)

    try:
        Atspi.init()
    except Exception as exc:
        print(f"CHECK5: NOT RUN — Atspi.init() failed: {exc}")
        sys.exit(2)

    deadline = time.monotonic() + args.timeout
    verdict = None
    all_seen: List[str] = []
    attempt = 0
    while time.monotonic() < deadline:
        attempt += 1
        try:
            desktop = Atspi.get_desktop(0)
        except Exception as exc:
            print(f"CHECK5: NOT RUN — Atspi.get_desktop(0) failed: {exc}")
            sys.exit(2)
        if desktop is None:
            print("CHECK5: NOT RUN — Atspi.get_desktop(0) returned None (no AT-SPI registry?)")
            sys.exit(2)

        roots: List[ObservedNode] = []
        all_seen = []
        try:
            n_apps = desktop.get_child_count()
        except Exception as exc:
            print(f"CHECK5: NOT RUN — desktop.get_child_count() failed: {exc}")
            sys.exit(2)

        for i in range(n_apps):
            try:
                app = desktop.get_child_at_index(i)
            except Exception:
                continue
            if app is None:
                continue
            try:
                app_name = app.get_name()
            except Exception:
                app_name = "<error>"
            if args.app_name not in app_name:
                continue
            app_observed = walk_for_check5(app, ["desktop"], all_seen, args.max_depth)
            if app_observed is not None:
                roots.append(app_observed)

        verdict = classify(expectation, roots)
        if verdict.verdict == "PASS":
            break
        time.sleep(args.poll_interval)

    assert verdict is not None  # the while loop above always runs at least once before a timeout

    print(f"CHECK5: {verdict.verdict}")
    print(f"fixture: {args.fixture}")
    print(f"attempt: {attempt}, timeout: {args.timeout}s")
    print(f"reason: {verdict.reason}")
    if verdict.observed_role is not None:
        print(f"observed role: {verdict.observed_role}")
    if verdict.observed_name is not None:
        print(f"observed name: {verdict.observed_name!r}")
        print(f"observed name (hex): {hex_lower(verdict.observed_name)}")
    if verdict.prohibited_outcome is not None:
        print(f"prohibited outcome: {verdict.prohibited_outcome}")
    print("full tree (role:name) seen during the last walk:")
    if not all_seen:
        print("  <none — no app matched --app-name, or it exposed no accessible children>")
    for line in all_seen:
        print("  " + line)

    if args.json:
        payload = {
            "fixture_id": args.fixture,
            "verdict": verdict.verdict,
            "reason": verdict.reason,
            "observed_role": verdict.observed_role,
            "observed_name": verdict.observed_name,
            "observed_name_hex": hex_lower(verdict.observed_name),
            "prohibited_outcome": verdict.prohibited_outcome,
            "walked_tree": all_seen,
        }
        with open(args.json, "w", encoding="utf-8") as f:
            json.dump(payload, f, indent=2)
            f.write("\n")

    sys.exit({"PASS": 0, "FAIL": 1}[verdict.verdict])


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--role", default=None, help="Round 0 mode: exact role to match")
    ap.add_argument("--name", default=None, help="Round 0 mode: exact name to match")
    ap.add_argument(
        "--expectations",
        default=None,
        help="Round 2 check 5 mode: path to round2-a11y-oracle's a11y_expectations.json",
    )
    ap.add_argument("--fixture", default=None, help="Round 2 check 5 mode: fixture id (e.g. F-A)")
    ap.add_argument(
        "--expect-source-digest",
        default=None,
        help="Round 2 check 5 mode (required): round2_textkit::output::expected_artifact_digest() "
        "— refuses the expectations file (usage error, exit 2) if its source_fixtures_digest "
        "disagrees, so a stale oracle cannot score a candidate (B2)",
    )
    ap.add_argument("--json", default=None, help="Round 2 check 5 mode: write the machine-readable verdict here")
    ap.add_argument("--app-name", default=None, help="only descend into apps whose name contains this substring")
    ap.add_argument("--max-depth", type=int, default=12)
    ap.add_argument("--timeout", type=float, default=20.0)
    ap.add_argument("--poll-interval", type=float, default=0.5)
    args = ap.parse_args()

    round0_mode = args.role is not None and args.name is not None
    check5_mode = args.expectations is not None and args.fixture is not None

    if round0_mode and check5_mode:
        ap.error("--role/--name (Round 0 mode) and --expectations/--fixture (check 5 mode) are mutually exclusive")
    if not round0_mode and not check5_mode:
        ap.error("either --role and --name, or --expectations and --fixture, must be given")
    if check5_mode and not args.app_name:
        ap.error("--app-name is required in check 5 mode, to scope the walk to the candidate's application")
    if check5_mode and not args.expect_source_digest:
        ap.error(
            "--expect-source-digest is required in check 5 mode (B2) — pass "
            "round2_textkit::output::expected_artifact_digest()"
        )

    try:
        import gi

        gi.require_version("Atspi", "2.0")
        from gi.repository import Atspi
    except Exception as exc:  # pragma: no cover - environment probe
        label = "READBACK" if round0_mode else "CHECK5"
        print(f"{label}: NOT RUN — could not import gi.repository.Atspi: {exc}")
        sys.exit(2)

    if round0_mode:
        run_round0(args, Atspi)
    else:
        run_check5(args, Atspi)


if __name__ == "__main__":
    main()
