//! A group's sign over a staff of one line: it spans the staff as boxed.

use std::path::Path;

use epiphany_cli::load;
use epiphany_engrave::{Engraver, PageGeometry};
use epiphany_layout_ir::{
    to_constrained, to_logical, ConstraintSolver, ResolvedLayoutIR, SolverConfig,
};

fn engrave(name: &str, xml: &str) -> ResolvedLayoutIR {
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::write(&path, xml).expect("written");
    let loaded = load(&path).expect("loads");
    Engraver::with_geometry(PageGeometry::default())
        .solve(
            &to_constrained(&to_logical(&loaded.reduced.score)),
            &SolverConfig::default(),
        )
        .layout
}

/// A one-line percussion part of one measure.
fn percussion() -> String {
    let attrs = "<attributes><divisions>1</divisions><time><beats>4</beats><beat-type>4</beat-type></time><clef><sign>percussion</sign></clef><staff-details><staff-lines>1</staff-lines></staff-details></attributes>";
    let note = "<note><unpitched><display-step>E</display-step><display-octave>4</display-octave></unpitched><duration>4</duration><voice>1</voice><type>whole</type></note>";
    format!("<measure number=\"1\">{attrs}{note}</measure>")
}

/// `parts` one-line parts in the groups given, each `(first, last, symbol)` of
/// the parts' indices.
fn score(parts: usize, groups: &[(usize, usize, &str)]) -> String {
    let mut list = String::new();
    for p in 0..parts {
        for (g, (first, _, symbol)) in groups.iter().enumerate() {
            if *first == p {
                list += &format!(
                    "<part-group type=\"start\" number=\"{n}\"><group-symbol>{symbol}</group-symbol></part-group>",
                    n = g + 1
                );
            }
        }
        list += &format!("<score-part id=\"P{p}\"><part-name>p{p}</part-name></score-part>");
        for (g, (_, last, _)) in groups.iter().enumerate() {
            if *last == p {
                list += &format!("<part-group type=\"stop\" number=\"{n}\"/>", n = g + 1);
            }
        }
    }
    let body: String = (0..parts)
        .map(|p| format!("<part id=\"P{p}\">{}</part>", percussion()))
        .collect();
    format!("<score-partwise version=\"4.0\"><part-list>{list}</part-list>{body}</score-partwise>")
}

fn boxes(layout: &ResolvedLayoutIR, starts: &str) -> Vec<(f32, f32, f32, f32)> {
    layout
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str().starts_with(starts))
        .map(|g| {
            let (x, y, b) = (g.position.x.0, g.position.y.0, g.bounding_box);
            (x + b.left.0, x + b.right.0, y + b.bottom.0, y + b.top.0)
        })
        .collect()
}

fn overlap(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> bool {
    a.1.min(b.1) > a.0.max(b.0) && a.3.min(b.3) > a.2.max(b.2)
}

fn lines(layout: &ResolvedLayoutIR) -> Vec<f32> {
    let mut lines: Vec<f32> = layout
        .strokes
        .iter()
        .filter(|s| {
            matches!(s.provenance.source, epiphany_core::TypedObjectId::Staff(_))
                && s.from.y == s.to.y
        })
        .map(|s| s.from.y.0)
        .collect();
    lines.sort_by(|a, b| b.total_cmp(a));
    lines
}

/// A bracket over one-line staves spans each as boxed, so its hooks stand off
/// the clefs: MuseScore's `sy` and `ey`, and Gould lets no characters collide
/// (p. 41).
#[test]
fn a_brackets_hooks_stand_clear_of_one_line_staves_clefs() {
    let layout = engrave("bracket_one.musicxml", &score(2, &[(0, 1, "bracket")]));
    let clefs = boxes(&layout, "unpitchedPercussionClef");
    let lines = lines(&layout);
    let (top, bottom) = (
        boxes(&layout, "bracketTop"),
        boxes(&layout, "bracketBottom"),
    );
    assert_eq!((top.len(), bottom.len()), (1, 1));
    assert!(
        (top[0].2 - (lines[0] + 2.0)).abs() < 0.1,
        "the top hook two spaces over the line: {:?}, {lines:?}",
        top[0]
    );
    assert!(
        (bottom[0].3 - (lines[1] - 2.0)).abs() < 0.1,
        "the bottom hook two spaces under the line: {:?}, {lines:?}",
        bottom[0]
    );
    for hook in top.iter().chain(&bottom) {
        for clef in &clefs {
            assert!(!overlap(*hook, *clef), "hook {hook:?} on clef {clef:?}");
        }
    }
}

/// So does a brace: it spans the boxes of its staves, not their lines.
#[test]
fn a_brace_spans_one_line_staves_as_boxed() {
    let layout = engrave("brace_one.musicxml", &score(2, &[(0, 1, "brace")]));
    let lines = lines(&layout);
    let brace = layout
        .glyphs
        .iter()
        .find(|g| g.glyph.as_str() == "brace")
        .expect("a brace");
    let bottom = brace.position.y.0;
    let scale = brace.transform.as_ref().expect("scaled").matrix[1][1];
    let top = bottom + scale * (brace.bounding_box.top.0 - brace.bounding_box.bottom.0);
    assert!(
        (bottom - (lines[1] - 2.0)).abs() < 0.1 && (top - (lines[0] + 2.0)).abs() < 0.1,
        "the brace from {bottom} to {top}, the lines {lines:?}"
    );
}

/// And the system's opening line, the two spaces past each outer staff's line.
#[test]
fn the_systems_opening_line_spans_one_line_staves_as_boxed() {
    let layout = engrave("opening_one.musicxml", &score(2, &[]));
    let lines = lines(&layout);
    let opening = layout
        .strokes
        .iter()
        .filter(|s| s.from.x == s.to.x && s.from.y != s.to.y)
        .map(|s| (s.from.y.0.min(s.to.y.0), s.from.y.0.max(s.to.y.0)))
        .max_by(|a, b| (a.1 - a.0).total_cmp(&(b.1 - b.0)))
        .expect("an opening line");
    assert!(
        (opening.0 - (lines[1] - 2.0)).abs() < 0.1 && (opening.1 - (lines[0] + 2.0)).abs() < 0.1,
        "the opening line {opening:?}, the lines {lines:?}"
    );
}

/// Two one-line staves at adjoining brackets: the lower hook of one and the
/// upper hook of the next stand clear of each other.
#[test]
fn adjoining_brackets_hooks_stand_clear_over_one_line_staves() {
    let layout = engrave(
        "brackets_adjoining.musicxml",
        &score(4, &[(0, 1, "bracket"), (2, 3, "bracket")]),
    );
    let (top, bottom) = (
        boxes(&layout, "bracketTop"),
        boxes(&layout, "bracketBottom"),
    );
    assert_eq!((top.len(), bottom.len()), (2, 2));
    let lower_top = top
        .iter()
        .min_by(|a, b| a.2.total_cmp(&b.2))
        .expect("the lower bracket's top hook");
    let upper_bottom = bottom
        .iter()
        .max_by(|a, b| a.3.total_cmp(&b.3))
        .expect("the upper bracket's bottom hook");
    assert!(
        upper_bottom.2 - lower_top.3 > 0.0,
        "the hooks {upper_bottom:?} and {lower_top:?}"
    );
}
