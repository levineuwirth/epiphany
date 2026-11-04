//! What a page's spread and its staves' distances read, and how far each rule
//! reaches, by hand-built scores whose page holds them to one reading.

use std::collections::BTreeMap;
use std::path::Path;

use epiphany_cli::load;
use epiphany_core::{StaffId, TypedObjectId};
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

const HEAD: &str = "<attributes><divisions>4</divisions><time><beats>4</beats><beat-type>4</beat-type></time><clef><sign>G</sign><line>2</line></clef></attributes>";

/// Each staff's bottom and top lines in the first system, top staff first.
fn staves(layout: &ResolvedLayoutIR) -> Vec<(f32, f32)> {
    let system = layout.systems().next().expect("a system");
    let mut lines: BTreeMap<StaffId, (f32, f32)> = BTreeMap::new();
    for &i in &system.primitives.strokes {
        let stroke = &layout.strokes[i as usize];
        if let TypedObjectId::Staff(staff) = stroke.provenance.source {
            if stroke.from.y == stroke.to.y {
                let y = stroke.from.y.0;
                let e = lines.entry(staff).or_insert((y, y));
                e.0 = e.0.min(y);
                e.1 = e.1.max(y);
            }
        }
    }
    let mut staves: Vec<(f32, f32)> = lines.into_values().collect();
    staves.sort_by(|a, b| b.1.total_cmp(&a.1));
    staves
}

/// The smaller gaps are raised level as far as the widest gap of the page, one
/// past its cap too, as `distributeStaves` raises them: a soloist's gap above a
/// piano whose own staves its ink holds apart.
#[test]
fn a_soloist_rises_level_with_a_braced_parts_held_gap() {
    let clefs = "<staves>2</staves><clef number=\"1\"><sign>G</sign><line>2</line></clef><clef number=\"2\"><sign>F</sign><line>4</line></clef>";
    let solo = format!(
        "<measure number=\"1\">{HEAD}<note><pitch><step>B</step><octave>4</octave></pitch><duration>16</duration><voice>1</voice><type>whole</type></note></measure>"
    );
    let piano = format!(
        "<measure number=\"1\"><attributes><divisions>4</divisions><time><beats>4</beats><beat-type>4</beat-type></time>{clefs}</attributes><note><pitch><step>C</step><octave>3</octave></pitch><duration>16</duration><voice>1</voice><type>whole</type><staff>1</staff></note><backup><duration>16</duration></backup><note><pitch><step>C</step><octave>5</octave></pitch><duration>16</duration><voice>5</voice><type>whole</type><staff>2</staff></note></measure>"
    );
    let xml = format!(
        "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>a</part-name></score-part><score-part id=\"P2\"><part-name>b</part-name></score-part></part-list><part id=\"P1\">{solo}</part><part id=\"P2\">{piano}</part></score-partwise>"
    );
    let layout = engrave("spread_past_cap.musicxml", &xml);
    let v = staves(&layout);
    let (upper, inside) = (v[0].0 - v[1].1, v[1].0 - v[2].1);
    assert!(
        inside > 8.0,
        "the piano's staves apart by their ink: {inside}"
    );
    assert!(
        upper / 1.1 >= inside - 0.01,
        "the soloist's gap, in its units, level with the piano's at least: {upper}, the piano's {inside}"
    );
}
