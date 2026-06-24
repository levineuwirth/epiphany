//! Agent I's acceptance harness (the merge gate for `epiphany-render-svg`,
//! `spec/PHASE2_QUICKSTART.md` §"Acceptance criteria per agent" → I).
//!
//! For each named fixture, drive the full v0 pipeline through the **stub solver**
//! (this phase's deliverable is the renderer-against-stub) and assert:
//!
//! * the SVG is well-formed (the in-crate checker *and*, when available, the
//!   system `xmllint` — a real external XML parser);
//! * every resolved glyph is drawn (one `<path>` per glyph; no silent drops) and
//!   traces back to its score-graph source via `data-prov`;
//! * a **golden-locked machine acceptance snapshot** (object / glyph / path /
//!   provenance / layer / per-class / hard-constraint counts + XML validity)
//!   matches a committed file — changes require an explicit golden update;
//! * the full SVG bytes match a committed golden — the renderer is deterministic.
//!
//! Regenerate goldens deliberately with `UPDATE_GOLDEN=1 cargo test -p
//! epiphany-render-svg --test acceptance`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use epiphany_core::Score;
use epiphany_layout_ir::{
    to_constrained, to_logical, ConstrainedLayoutIR, ConstraintSolver, SolverConfig, StubSolver,
};
use epiphany_render_svg::{check_well_formed, render, GlyphClass, RenderOptions, RenderOutput};

/// The fixtures Agent I's acceptance names, each with a fixed seed so the score
/// — and therefore the layout and SVG — is deterministic.
fn fixtures() -> Vec<(&'static str, Score)> {
    vec![
        (
            "ten_measure_single_staff",
            epiphany_testkit::fixtures::ten_measure_single_staff(0x000A_11CE),
        ),
        (
            "valid_score_rich",
            epiphany_core::generators::valid_score_rich(0x5EED),
        ),
    ]
}

fn pipeline(score: &Score) -> (ConstrainedLayoutIR, RenderOutput) {
    let constrained = to_constrained(&to_logical(score));
    let layout = StubSolver
        .solve(&constrained, &SolverConfig::default())
        .layout;
    let out = render(&layout, &RenderOptions::default());
    (constrained, out)
}

/// A deterministic, human-diffable serialization of the machine acceptance
/// snapshot.
fn snapshot_text(fixture: &str, constrained: &ConstrainedLayoutIR, out: &RenderOutput) -> String {
    let mut s = String::new();
    s.push_str(&format!("fixture={fixture} solver=stub\n"));
    s.push_str(&format!("glyph_count={}\n", out.stats.glyph_count));
    s.push_str(&format!("path_count={}\n", out.stats.path_count));
    s.push_str(&format!(
        "fallback_rect_count={}\n",
        out.stats.fallback_rect_count
    ));
    s.push_str(&format!(
        "provenance_count={}\n",
        out.stats.provenance_count
    ));
    s.push_str(&format!("layer_count={}\n", out.stats.layer_count));
    s.push_str(&format!(
        "hard_constraint_count={}\n",
        constrained.constraints.len()
    ));
    s.push_str(&format!(
        "xml_well_formed={}\n",
        check_well_formed(&out.svg).is_ok()
    ));
    let vb = out.stats.view_box;
    s.push_str(&format!(
        "view_box=[{} {} {} {}]\n",
        vb[0], vb[1], vb[2], vb[3]
    ));
    let labelled: BTreeMap<&str, usize> = out
        .stats
        .class_counts
        .iter()
        .map(|(c, n)| (c.token(), *n))
        .collect();
    s.push_str("class_counts:\n");
    for (token, n) in labelled {
        s.push_str(&format!("  {token}={n}\n"));
    }
    s
}

fn golden_path(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests");
    p.push("golden");
    p.push(name);
    p
}

/// Compares `actual` to the committed golden at `tests/golden/<name>`, or writes
/// it when `UPDATE_GOLDEN` is set.
fn assert_golden(name: &str, actual: &str) {
    let path = golden_path(name);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "missing golden {} ({e}); regenerate with UPDATE_GOLDEN=1",
            path.display()
        )
    });
    assert_eq!(
        actual,
        expected,
        "golden mismatch for {}; if intended, rerun with UPDATE_GOLDEN=1",
        path.display()
    );
}

#[test]
fn fixtures_render_to_golden_locked_svg_and_snapshot() {
    for (fixture, score) in fixtures() {
        let (constrained, out) = pipeline(&score);

        // Structural invariants (would fail if the renderer stubbed/dropped work).
        assert!(out.stats.glyph_count > 0, "{fixture}: nothing was laid out");
        assert_eq!(
            out.stats.path_count + out.stats.fallback_rect_count,
            out.stats.glyph_count,
            "{fixture}: every glyph must be drawn (path or fallback), none dropped"
        );
        assert_eq!(
            out.stats.provenance_count, out.stats.glyph_count,
            "{fixture}: every drawn glyph must carry a provenance trace"
        );
        let class_sum: usize = out.stats.class_counts.values().sum();
        assert_eq!(
            class_sum, out.stats.glyph_count,
            "{fixture}: classes partition glyphs"
        );

        // The in-crate well-formedness gate.
        check_well_formed(&out.svg)
            .unwrap_or_else(|e| panic!("{fixture}: emitted SVG is not well-formed: {e}"));

        // Provenance survival: every laid-out glyph's stable id appears as a
        // data-prov trace in the SVG text.
        for g in &out_glyph_ids(&score) {
            assert!(
                out.svg.contains(&format!("data-prov=\"{g:032x}\"")),
                "{fixture}: provenance {g:032x} did not survive into the SVG"
            );
        }

        // Golden locks: the machine snapshot and the full SVG bytes.
        assert_golden(
            &format!("{fixture}.stub.snapshot.txt"),
            &snapshot_text(fixture, &constrained, &out),
        );
        assert_golden(&format!("{fixture}.stub.svg"), &out.svg);
    }
}

/// The stable layout-object ids the pipeline lays out for a score (used to check
/// provenance survival into the SVG).
fn out_glyph_ids(score: &Score) -> Vec<u128> {
    let layout = StubSolver
        .solve(
            &to_constrained(&to_logical(score)),
            &SolverConfig::default(),
        )
        .layout;
    layout
        .glyphs
        .iter()
        .map(|g| g.provenance.stable_id.0)
        .collect()
}

#[test]
fn the_renderer_draws_real_bravura_curves_not_placeholders() {
    // Non-vacuity: the SVG must contain genuine outline data (cubic bezier 'C'
    // commands from the Bravura CFF outlines), not just rectangles or moves.
    let (_, out) = pipeline(&epiphany_testkit::fixtures::ten_measure_single_staff(
        0x000A_11CE,
    ));
    let curve_paths = out
        .svg
        .lines()
        .filter(|l| l.contains("<path") && l.contains('C'))
        .count();
    assert!(
        curve_paths > 0,
        "expected genuine Bravura bezier outlines, found none"
    );
}

#[test]
fn classes_are_non_trivial() {
    // The stub pipeline maps several object kinds to several glyph classes; the
    // snapshot should not collapse to a single class (which would suggest the
    // classifier or the pipeline went vacuous).
    let (_, out) = pipeline(&epiphany_core::generators::valid_score_rich(0x5EED));
    let nonzero = out.stats.class_counts.values().filter(|&&n| n > 0).count();
    assert!(
        nonzero >= 2,
        "expected several glyph classes, got {nonzero}"
    );
    // Sanity: the classifier knows the standard categories.
    assert_eq!(GlyphClass::of("gClef"), GlyphClass::Clef);
    assert_eq!(GlyphClass::of("noteheadBlack"), GlyphClass::Notehead);
}

/// Best-effort cross-check with the system `xmllint` (a real XML parser). When
/// `xmllint` is absent the in-crate checker remains the gate; when present, the
/// "XML-validates" claim rests on an external parser too.
#[test]
fn svg_validates_under_xmllint_when_available() {
    use std::io::Write;
    use std::process::Command;

    let (_, out) = pipeline(&epiphany_testkit::fixtures::ten_measure_single_staff(
        0x000A_11CE,
    ));

    let mut tmp = std::env::temp_dir();
    tmp.push("epiphany_render_svg_xmllint_check.svg");
    let mut f = match std::fs::File::create(&tmp) {
        Ok(f) => f,
        Err(_) => return, // can't write a temp file; skip
    };
    f.write_all(out.svg.as_bytes()).unwrap();
    drop(f);

    match Command::new("xmllint").arg("--noout").arg(&tmp).status() {
        Ok(status) => assert!(
            status.success(),
            "xmllint rejected the emitted SVG as malformed"
        ),
        Err(_) => {
            // xmllint not installed; the in-crate checker already gated this run.
            eprintln!("note: xmllint not available; relied on the in-crate checker");
        }
    }
    let _ = std::fs::remove_file(&tmp);
}
