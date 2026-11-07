//! How far each rule of the vertical spacing reaches: the clauses of the staves'
//! distances, the page's spread, the rests and the centring between staves, each
//! on a hand-built score whose page reads only that clause.

use std::path::Path;

use epiphany_cli::load;
use epiphany_core::TypedObjectId;
use epiphany_engrave::{Engraver, PageGeometry};
use epiphany_layout_ir::{
    to_constrained, to_logical, ConstraintSolver, ResolvedLayoutIR, ResolvedSystem, SolverConfig,
    StaffSpace,
};

fn engrave_on(name: &str, xml: &str, geometry: PageGeometry) -> ResolvedLayoutIR {
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::write(&path, xml).expect("written");
    let loaded = load(&path).expect("loads");
    Engraver::with_geometry(geometry)
        .solve(
            &to_constrained(&to_logical(&loaded.reduced.score)),
            &SolverConfig::default(),
        )
        .layout
}

const TREBLE: &str = "<clef><sign>G</sign><line>2</line></clef>";

fn head() -> String {
    format!("<attributes><divisions>1</divisions><time><beats>4</beats><beat-type>4</beat-type></time>{TREBLE}</attributes>")
}

fn whole(step: &str, o: u8) -> String {
    format!("<note><pitch><step>{step}</step><octave>{o}</octave></pitch><duration>4</duration><voice>1</voice><type>whole</type></note>")
}

fn wedge(kind: &str) -> String {
    format!("<direction placement=\"below\"><direction-type><wedge type=\"{kind}\"/></direction-type><staff>1</staff></direction>")
}

fn note(step: &str, octave: u8, d: u8, kind: &str) -> String {
    format!("<note><pitch><step>{step}</step><octave>{octave}</octave></pitch><duration>{d}</duration><voice>1</voice><type>{kind}</type></note>")
}

fn score(parts: &[String], list: Option<String>) -> String {
    let dflt: String = (0..parts.len())
        .map(|i| {
            format!(
                "<score-part id=\"P{0}\"><part-name>P{0}</part-name></score-part>",
                i + 1
            )
        })
        .collect();
    let body: String = parts
        .iter()
        .enumerate()
        .map(|(i, p)| format!("<part id=\"P{}\">{p}</part>", i + 1))
        .collect();
    format!(
        "<score-partwise version=\"4.0\"><part-list>{}</part-list>{body}</score-partwise>",
        list.unwrap_or(dflt)
    )
}

/// Each staff's bottom and top lines in a system, top staff first.
fn lines(l: &ResolvedLayoutIR, s: &ResolvedSystem) -> Vec<(f32, f32)> {
    let mut m: std::collections::BTreeMap<epiphany_core::StaffId, (f32, f32)> = Default::default();
    for &i in &s.primitives.strokes {
        let t = &l.strokes[i as usize];
        if let TypedObjectId::Staff(st) = t.provenance.source {
            if t.from.y == t.to.y {
                let e = m.entry(st).or_insert((t.from.y.0, t.from.y.0));
                e.0 = e.0.min(t.from.y.0);
                e.1 = e.1.max(t.from.y.0);
            }
        }
    }
    let mut v: Vec<_> = m.into_values().collect();
    v.sort_by(|a, b| b.1.total_cmp(&a.1));
    v
}

fn page(h: f32) -> PageGeometry {
    let mut g = PageGeometry::default();
    g.size.height = StaffSpace(h);
    g.margins.top = StaffSpace(6.0);
    g.margins.bottom = StaffSpace(6.0);
    g
}

/// A system's hairpin legs' boxes: left, bottom, right, top.
fn spanner_strokes(layout: &ResolvedLayoutIR, system: &ResolvedSystem) -> Vec<[f32; 4]> {
    system
        .primitives
        .strokes
        .iter()
        .filter_map(|&i| {
            let s = &layout.strokes[i as usize];
            matches!(s.provenance.source, TypedObjectId::Spanner(_)).then(|| {
                let h = s.thickness.0 * 0.5;
                [
                    s.from.x.0.min(s.to.x.0) - h,
                    s.from.y.0.min(s.to.y.0) - h,
                    s.from.x.0.max(s.to.x.0) + h,
                    s.from.y.0.max(s.to.y.0) + h,
                ]
            })
        })
        .collect()
}

fn grand_xml(parts: &[String]) -> String {
    let clefs = "<staves>2</staves><clef number=\"1\"><sign>G</sign><line>2</line></clef><clef number=\"2\"><sign>F</sign><line>4</line></clef>";
    let body: String = parts
        .iter()
        .enumerate()
        .map(|(m, c)| {
            let a = if m == 0 {
                format!("<attributes><divisions>1</divisions><time><beats>4</beats><beat-type>4</beat-type></time>{clefs}</attributes>")
            } else {
                String::new()
            };
            format!("<measure number=\"{}\">{a}{c}</measure>", m + 1)
        })
        .collect();
    format!("<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>A</part-name></score-part></part-list><part id=\"P1\">{body}</part></score-partwise>")
}

fn gn(step: &str, o: u8, d: u8, kind: &str, voice: u8, staff: u8, ly: Option<&str>) -> String {
    let l = ly.map_or(String::new(), |w| {
        format!("<lyric number=\"1\"><syllabic>single</syllabic><text>{w}</text></lyric>")
    });
    format!("<note><pitch><step>{step}</step><octave>{o}</octave></pitch><duration>{d}</duration><voice>{voice}</voice><type>{kind}</type><staff>{staff}</staff>{l}</note>")
}

fn glyph_box(layout: &ResolvedLayoutIR, name: &str) -> [f32; 4] {
    let g = layout
        .glyphs
        .iter()
        .find(|g| g.glyph.as_str() == name)
        .expect(name);
    let (x, y, b) = (g.position.x.0, g.position.y.0, g.bounding_box);
    [x + b.left.0, y + b.bottom.0, x + b.right.0, y + b.top.0]
}

const BK: &str = "<backup><duration>4</duration></backup>";
const P2: &str = "<direction><direction-type><dynamics><p/></dynamics></direction-type><staff>2</staff></direction>";
const P1: &str = "<direction><direction-type><dynamics><p/></dynamics></direction-type><staff>1</staff></direction>";

/// A split hairpin's piece lifted off its place by a low note sets the staff
/// below by its mouth where it now stands.
#[test]
fn a_lifted_piece_sets_the_staff_below_by_its_mouth() {
    let upper = format!("<measure number=\"1\">{}{}{}</measure><measure number=\"2\"><print new-system=\"yes\"/>{}{}</measure>",
        head(), wedge("crescendo"), note("C", 3, 4, "whole"), note("B", 4, 2, "half") + &wedge("stop"), note("B", 4, 2, "half"));
    let lower = format!(
        "<measure number=\"1\">{}{}{}{}{}</measure><measure number=\"2\">{}</measure>",
        head(),
        note("B", 4, 1, "quarter"),
        note("B", 4, 1, "quarter"),
        note("B", 4, 1, "quarter"),
        note("C", 6, 1, "quarter"),
        note("B", 4, 4, "whole")
    );
    let mut g = PageGeometry::default();
    g.size.height = epiphany_layout_ir::StaffSpace(0.0);
    let layout = engrave_on("lifted_outline.musicxml", &score(&[upper, lower], None), g);
    let system = layout.systems().next().expect("a first system");
    let legs = spanner_strokes(&layout, system);
    assert_eq!(legs.len(), 2, "the piece's two legs: {legs:?}");
    let ls = lines(&layout, system);
    // the mouth is the legs' lowest edge; the lower staff's top-most ink below it across: its C6's head at the piece's end
    let mouth_low = legs.iter().map(|r| r[1]).fold(f32::INFINITY, f32::min);
    let right = legs.iter().map(|r| r[2]).fold(f32::NEG_INFINITY, f32::max);
    let mut least = f32::INFINITY;
    for &i in &system.primitives.glyphs {
        let g = &layout.glyphs[i as usize];
        let (x, y, b) = (g.position.x.0, g.position.y.0, g.bounding_box);
        if y < ls[1].1 + 8.0
            && y > ls[1].0 - 1.0
            && g.glyph.as_str() == "noteheadBlack"
            && x + b.left.0 < right + 0.25
            && x + b.right.0 > legs[0][0] - 0.25
        {
            least = least.min(mouth_low - (y + b.top.0));
        }
    }
    assert!(
        ls[0].0 - mouth_low > 3.0,
        "the piece stands lifted from its staff: {}",
        ls[0].0 - mouth_low
    );
    assert!(
        (0.49..0.75).contains(&least),
        "the mouth decides the gap, 0.5 clear: {least}"
    );
}

/// A page's spread stops where a lifted piece's mouth meets the bottom margin.
#[test]
fn a_lifted_piece_ends_the_pages_spread_at_its_mouth() {
    let upper = format!("<measure number=\"1\">{}{}</measure><measure number=\"2\"><print new-page=\"yes\"/>{}</measure>", head(), note("B", 4, 4, "whole"), note("B", 4, 4, "whole"));
    let lower = format!("<measure number=\"1\">{}{}{}</measure><measure number=\"2\"><print new-page=\"yes\"/>{}{}</measure>",
        head(), wedge("crescendo"), note("A", 1, 4, "whole"), note("B", 4, 2, "half") + &wedge("stop"), note("B", 4, 2, "half"));
    let mut g = PageGeometry::default();
    g.size.height = epiphany_layout_ir::StaffSpace(32.0);
    g.margins.top = epiphany_layout_ir::StaffSpace(2.0);
    g.margins.bottom = epiphany_layout_ir::StaffSpace(2.0);
    let frame = g.page_frame(0);
    let margin = frame.origin.y.0 + 2.0;
    let layout = engrave_on("lifted_extent.musicxml", &score(&[upper, lower], None), g);
    let system = layout.systems().next().expect("a first system");
    let legs = spanner_strokes(&layout, system);
    assert_eq!(legs.len(), 2, "{legs:?}");
    let mouth_low = legs.iter().map(|r| r[1]).fold(f32::INFINITY, f32::min);
    assert!(
        (mouth_low - margin).abs() < 0.02,
        "the mouth meets the margin: {mouth_low} against {margin}"
    );
}

/// The other staff's syllable bounds the p exactly, by the distance the
/// centring moves it: the p stands its least distance under the syllable.
#[test]
fn a_centred_dynamic_stops_its_least_distance_from_the_other_staffs_syllable() {
    let up: String = ["aaaaaa", "bbbbbb", "cccccc", "dddddd"]
        .iter()
        .map(|w| gn("A", 4, 1, "quarter", 1, 1, Some(w)))
        .collect();
    let lo: String = ["eeeeee", "ffffff", "gggggg", "hhhhhh"]
        .iter()
        .map(|w| gn("C", 3, 1, "quarter", 5, 2, Some(w)))
        .collect();
    let xml = grand_xml(&[format!("{up}{BK}{P2}{lo}")]);
    let layout = engrave_on("centred_syllable.musicxml", &xml, PageGeometry::default());
    let d = glyph_box(&layout, "dynamicPiano");
    let t = layout
        .texts
        .iter()
        .find(|t| t.text == "aaaaaa")
        .expect("the syllable");
    let bottom = t.origin.y.0 + t.bounds.bottom.0;
    let gap = bottom - d[3];
    let s = layout.systems().next().unwrap();
    let ls = lines(&layout, s);
    let mid = (ls[0].0 + ls[1].1) * 0.5;
    assert!(
        (d[1] + d[3]) * 0.5 < mid + 0.02,
        "the p stands at or under the gap's middle: {d:?} {mid}"
    );
    assert!(
        (0.49..0.51).contains(&gap),
        "the p stands its least distance under the syllable: gap {gap}, p {d:?}, mid {mid}"
    );
}

/// Cl-swap, own side: a p under a fermata on a sung lower staff (the fermata's
/// place from the session's test) moves toward the upper staff exactly as far
/// as the fermata lets it.
#[test]
fn a_centred_dynamic_with_nothing_between_stands_at_the_gaps_middle() {
    let up: String = (0..4)
        .map(|_| gn("C", 5, 1, "quarter", 1, 1, None))
        .collect();
    let lo: String = (0..4)
        .map(|_| gn("C", 3, 1, "quarter", 5, 2, None))
        .collect();
    let xml = grand_xml(&[format!("{P1}{up}{BK}{lo}")]);
    let layout = engrave_on("centred_clear.musicxml", &xml, PageGeometry::default());
    let d = glyph_box(&layout, "dynamicPiano");
    let s = layout.systems().next().unwrap();
    let ls = lines(&layout, s);
    let mid = (ls[0].0 + ls[1].1) * 0.5;
    assert!(ls[0].0 - ls[1].1 > 5.0, "room: {ls:?}");
    assert!(
        ((d[1] + d[3]) * 0.5 - mid).abs() < 0.6,
        "the p at the middle of the gap: {d:?} against {mid}"
    );
}

/// Another system's syllables at the same x do not hold the p.
#[test]
fn a_centred_dynamic_reads_only_its_own_systems_family() {
    let up1: String = gn(
        "A",
        4,
        4,
        "whole",
        1,
        1,
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
    );
    let lo1: String = gn("C", 3, 4, "whole", 5, 2, None);
    let up2: String = format!(
        "<print new-system=\"yes\"/>{}",
        gn("A", 4, 4, "whole", 1, 1, None)
    );
    let lo2: String = gn("C", 3, 4, "whole", 5, 2, None);
    let xml = grand_xml(&[format!("{up1}{BK}{lo1}"), format!("{P1}{up2}{BK}{lo2}")]);
    let layout = engrave_on(
        "centred_other_system.musicxml",
        &xml,
        PageGeometry::default(),
    );
    let d = glyph_box(&layout, "dynamicPiano");
    let s = layout.systems().nth(1).expect("a second system");
    let ls = lines(&layout, s);
    let mid = (ls[0].0 + ls[1].1) * 0.5;
    assert!(
        ((d[1] + d[3]) * 0.5 - mid).abs() < 0.6,
        "the p stands about the gap's middle: {d:?} against {mid}, lines {ls:?}"
    );
}

/// A gap between systems stops at its cap, 32, however wide the staff gaps
/// beside it stand.
#[test]
fn a_gap_between_systems_stops_at_its_cap() {
    // low notes on the upper staff force a wide gap between staves, so the levelling would raise the system gap past its cap
    let u = format!("<measure number=\"1\">{}{}</measure><measure number=\"2\"><print new-system=\"yes\"/>{}</measure>", head(), whole("A", 1), whole("A", 1));
    let d = format!(
        "<measure number=\"1\">{}{}</measure><measure number=\"2\">{}</measure>",
        head(),
        whole("B", 4),
        whole("B", 4)
    );
    let l = engrave_on("syscap.musicxml", &score(&[u, d], None), page(160.0));
    let v: Vec<_> = l.systems().collect();
    assert_eq!(v.len(), 2);
    let (a, b) = (lines(&l, v[0]), lines(&l, v[1]));
    let sys = a[1].0 - b[0].1;
    assert!(
        (sys - 32.0).abs() < 0.01,
        "the gap between systems at its cap: {sys}; staff gaps {} {}",
        a[0].0 - a[1].1,
        b[0].0 - b[1].1
    );
}

/// A gap at a bracket's edge stops at its cap in the spread's units,
/// MuseScore's maxStaffSpread undivided: 24 on the page.
#[test]
fn a_gap_at_a_brackets_edge_stops_at_twenty_units() {
    let list = "<part-group type=\"start\" number=\"1\"><group-symbol>bracket</group-symbol></part-group><score-part id=\"P1\"><part-name>a</part-name></score-part><score-part id=\"P2\"><part-name>b</part-name></score-part><part-group type=\"stop\" number=\"1\"/><score-part id=\"P3\"><part-name>c</part-name></score-part>";
    let plain = |s: &str, o| format!("<measure number=\"1\">{}{}</measure>", head(), whole(s, o));
    // the gap inside the bracket forced wide by ink; the edge gap raised level with it, to its cap
    let l = engrave_on(
        "edgecap.musicxml",
        &score(
            &[plain("C", 0), plain("B", 4), plain("B", 4)],
            Some(list.to_owned()),
        ),
        page(120.0),
    );
    let s = l.systems().next().unwrap();
    let v = lines(&l, s);
    let (inside, edge) = (v[0].0 - v[1].1, v[1].0 - v[2].1);
    assert!(
        inside >= 19.99,
        "the first gap, at or past its cap: {inside}"
    );
    assert!(
        (edge - 24.0).abs() < 0.01,
        "the edge's gap at 20 units: {edge}, the first {inside}"
    );
}

/// A page's first staff stands lower than its border where its ink reaches past
/// 7 above it.
#[test]
fn a_first_staff_stands_below_its_ink() {
    let u = format!(
        "<measure number=\"1\">{}{}</measure>",
        head(),
        whole("A", 7)
    );
    let l = engrave_on("inktop.musicxml", &score(&[u], None), page(120.0));
    let g = page(120.0);
    let f = g.page_frame(0);
    let top = f.origin.y.0 + f.size.height.0 - 6.0;
    let s = l.systems().next().unwrap();
    let hi = s.bounding_box.origin.y.0 + s.bounding_box.size.height.0;
    assert!(
        (hi - top).abs() < 0.02,
        "the ink at the top margin: {hi} against {top}"
    );
    assert!(
        lines(&l, s)[0].1 < top - 7.0 - 0.5,
        "the top line under the border"
    );
}

/// A further system goes on the page only where its ink keeps the margin, when
/// the ink reaches past 7 under its last staff.
#[test]
fn a_further_system_goes_on_only_where_its_ink_fits() {
    // one staff, the second system's note far under it; the page holds its line and border but not its ink
    let u = format!("<measure number=\"1\">{}{}</measure><measure number=\"2\"><print new-system=\"yes\"/>{}</measure>", head(), whole("B", 4), whole("A", 1));
    let l = engrave_on("furtherink.musicxml", &score(&[u], None), page(43.0));
    assert_eq!(
        l.pages.len(),
        2,
        "its ink sends the system over: {:?}",
        l.pages.iter().map(|p| p.systems.len()).collect::<Vec<_>>()
    );
}

/// A measure rest centres in its own staff's free space: another staff's clef
/// change closing the measure does not narrow it.
#[test]
fn a_measure_rest_ignores_another_staffs_clef() {
    let rest = "<note><rest measure=\"yes\"/><duration>4</duration><voice>1</voice></note>";
    let u = format!("<measure number=\"1\">{}{}</measure><measure number=\"2\">{rest}</measure><measure number=\"3\">{}</measure>", head(), whole("B", 4), whole("B", 4));
    let d = format!("<measure number=\"1\">{}{}</measure><measure number=\"2\">{rest}<attributes><clef><sign>F</sign><line>4</line></clef></attributes></measure><measure number=\"3\">{}</measure>", head(), whole("B", 4), whole("D", 3));
    let l = engrave_on(
        "otherclef.musicxml",
        &score(&[u, d], None),
        PageGeometry::default(),
    );
    let mut rests: Vec<_> = l
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str() == "restWhole")
        .map(|g| g.position.x.0)
        .collect();
    rests.sort_by(f32::total_cmp);
    let bars: Vec<f32> = {
        let mut b: Vec<f32> = l
            .glyphs
            .iter()
            .filter(|g| g.glyph.as_str() == "barlineSingle")
            .map(|g| g.position.x.0 + g.bounding_box.right.0 * 0.5)
            .collect();
        b.sort_by(f32::total_cmp);
        b.dedup_by(|a, b| (*a - *b).abs() < 0.01);
        b
    };
    assert_eq!(rests.len(), 2);
    assert!(
        rests[1] - rests[0] > 0.5,
        "the clef's staff's rest stands left of the other's: {rests:?} bars {bars:?}"
    );
    let w = l
        .glyphs
        .iter()
        .find(|g| g.glyph.as_str() == "restWhole")
        .unwrap()
        .bounding_box;
    let upper_mid = rests[1] + (w.left.0 + w.right.0) * 0.5;
    assert!(
        (upper_mid - (bars[0] + bars[1]) * 0.5).abs() < 0.2,
        "the staff with no clef centres between its barlines: {upper_mid} in {bars:?}"
    );
}

/// The staff below a staff of one line stands the floor below its box, two
/// spaces under the line.
#[test]
fn a_staff_below_a_line_stands_the_floor_below_its_box() {
    let perc = "<attributes><divisions>1</divisions><time><beats>4</beats><beat-type>4</beat-type></time><clef><sign>percussion</sign></clef><staff-details><staff-lines>1</staff-lines></staff-details></attributes>";
    let n = "<note><unpitched><display-step>E</display-step><display-octave>4</display-octave></unpitched><duration>4</duration><voice>1</voice><type>whole</type></note>";
    let a = format!("<measure number=\"1\">{perc}{n}</measure>");
    let b = format!(
        "<measure number=\"1\">{}{}</measure>",
        head(),
        whole("B", 4)
    );
    let mut g = PageGeometry::default();
    g.size.height = StaffSpace(0.0);
    let l = engrave_on("onebox.musicxml", &score(&[a, b], None), g);
    let s = l.systems().next().unwrap();
    let v = lines(&l, s);
    assert!(
        (v[0].0 - v[1].1 - 6.0).abs() < 0.01,
        "line to top line: {}",
        v[0].0 - v[1].1
    );
}

/// A half rest under its staff takes its ledger line as one over it does.
#[test]
fn a_half_rest_under_its_staff_is_ledgered() {
    let m = "<note><pitch><step>C</step><octave>4</octave></pitch><duration>2</duration><voice>1</voice><type>half</type><stem>up</stem></note><note><pitch><step>C</step><octave>4</octave></pitch><duration>2</duration><voice>1</voice><type>half</type><stem>up</stem></note><backup><duration>4</duration></backup><note><rest/><duration>2</duration><voice>2</voice><type>half</type></note><note><rest/><duration>2</duration><voice>2</voice><type>half</type></note>";
    let u = format!("<measure number=\"1\">{}{m}</measure>", head());
    let l = engrave_on(
        "halfunder.musicxml",
        &score(&[u], None),
        PageGeometry::default(),
    );
    let n: Vec<_> = l
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str().starts_with("restHalf"))
        .map(|g| g.glyph.as_str().to_owned())
        .collect();
    assert!(n.iter().all(|x| x == "restHalfLegerLine"), "{n:?}");
}

/// A whole rest under its staff takes its ledger line.
#[test]
fn a_whole_rest_under_its_staff_is_ledgered() {
    let m = "<note><pitch><step>C</step><octave>4</octave></pitch><duration>4</duration><voice>1</voice><type>whole</type></note><backup><duration>4</duration></backup><note><rest measure=\"yes\"/><duration>4</duration><voice>2</voice></note>";
    let u = format!("<measure number=\"1\">{}{m}</measure>", head());
    let l = engrave_on(
        "wholeunder.musicxml",
        &score(&[u], None),
        PageGeometry::default(),
    );
    assert!(
        l.glyphs
            .iter()
            .any(|g| g.glyph.as_str() == "restWholeLegerLine"),
        "the whole rest under the staff"
    );
}

/// A page's spread stops where its ink meets the bottom margin, the last staff
/// then standing further than its border above it.
#[test]
fn a_pages_spread_stops_where_its_ink_meets_the_margin() {
    let u = format!(
        "<measure number=\"1\">{}{}</measure>",
        head(),
        whole("B", 4)
    );
    let d = format!(
        "<measure number=\"1\">{}{}</measure>",
        head(),
        whole("A", 1)
    );
    let g = page(43.0);
    let f = g.page_frame(0);
    let margin = f.origin.y.0 + 6.0;
    let l = engrave_on("spreadink.musicxml", &score(&[u, d], None), g);
    let s = l.systems().next().unwrap();
    let v = lines(&l, s);
    assert!(v[0].0 - v[1].1 > 4.5, "the page spread its gap: {v:?}");
    assert!(
        (s.bounding_box.origin.y.0 - margin).abs() < 0.02,
        "the ink at the margin: {} against {margin}",
        s.bounding_box.origin.y.0
    );
    assert!(
        v[1].0 - margin > 7.5,
        "the last staff further than its border above it: {}",
        v[1].0 - margin
    );
}

/// A gap between systems raised toward a wider staff gap beside it stops at its
/// own cap.
#[test]
fn the_raise_toward_a_wide_staff_gap_stops_at_the_system_cap() {
    // the first staff's lowest note holds its staff gap past 32 by ink; a third staff's height of room besides
    let u = format!("<measure number=\"1\">{}{}</measure><measure number=\"2\"><print new-system=\"yes\"/>{}</measure>", head(), whole("C", 0), whole("B", 4));
    let bass = head().replace(TREBLE, "<clef><sign>F</sign><line>4</line></clef>");
    let m = format!(
        "<measure number=\"1\">{}{}</measure><measure number=\"2\">{}</measure>",
        bass,
        whole("C", 9),
        whole("B", 2)
    );
    let l = engrave_on("raisecap.musicxml", &score(&[u, m], None), page(240.0));
    let v: Vec<_> = l.systems().collect();
    assert_eq!(v.len(), 2);
    let (a, b) = (lines(&l, v[0]), lines(&l, v[1]));
    let (staff, sys) = (a[0].0 - a[1].1, a[1].0 - b[0].1);
    assert!(staff > 32.0, "the staff gap past the system cap: {staff}");
    assert!(
        sys <= 32.01,
        "the gap between systems at most its cap: {sys}, the staff gap {staff}"
    );
}

/// A syllable under a system's last staff keeps 1.5 from the next system's ink
/// where they meet across, as from a next staff's.
#[test]
fn a_syllable_keeps_its_room_from_the_next_systems_ink() {
    let ly = "<note><pitch><step>B</step><octave>4</octave></pitch><duration>4</duration><voice>1</voice><type>whole</type><lyric number=\"1\"><syllabic>single</syllabic><text>lalalalalalalalalalala</text></lyric></note>";
    let u = format!("<measure number=\"1\">{}{ly}</measure><measure number=\"2\"><print new-system=\"yes\"/>{}</measure>", head(), whole("A", 6));
    let mut g = PageGeometry::default();
    g.size.height = StaffSpace(0.0);
    let l = engrave_on("syllable_system.musicxml", &score(&[u], None), g);
    let t = l
        .texts
        .iter()
        .find(|t| t.text.starts_with("lala"))
        .expect("the syllable");
    let bottom = t.origin.y.0 + t.bounds.bottom.0;
    let v: Vec<_> = l.systems().collect();
    let top = v[1].bounding_box.origin.y.0 + v[1].bounding_box.size.height.0;
    let (a, b) = (lines(&l, v[0]), lines(&l, v[1]));
    assert!(
        (bottom - top - 1.5).abs() < 0.01 && a[0].0 - b[0].1 > 8.6,
        "1.5 from the next system's ink: {} (line distance {})",
        bottom - top,
        a[0].0 - b[0].1
    );
}

/// A pitched grace on a staff of one line stands where its pitched principal of
/// the same pitch stands, its ledger lines as that note's, as the rule states.
#[test]
fn a_pitched_grace_on_one_line_stands_as_its_note_does() {
    let attrs = "<attributes><divisions>1</divisions><time><beats>4</beats><beat-type>4</beat-type></time><clef><sign>G</sign><line>2</line></clef><staff-details><staff-lines>1</staff-lines></staff-details></attributes>";
    let grace = "<note><grace slash=\"yes\"/><pitch><step>E</step><octave>4</octave></pitch><voice>1</voice><type>eighth</type></note>";
    let u = format!("<measure number=\"1\">{attrs}{grace}<note><pitch><step>E</step><octave>4</octave></pitch><duration>4</duration><voice>1</voice><type>whole</type></note></measure>");
    let mut g = PageGeometry::default();
    g.size.height = StaffSpace(0.0);
    let l = engrave_on("pitched_grace.musicxml", &score(&[u], None), g);
    let head = l
        .glyphs
        .iter()
        .find(|g| g.glyph.as_str().starts_with("notehead"))
        .expect("the principal");
    let hy = head.position.y.0 + (head.bounding_box.bottom.0 + head.bounding_box.top.0) * 0.5;
    let t = l
        .texts
        .iter()
        .min_by(|a, b| a.origin.x.0.total_cmp(&b.origin.x.0))
        .expect("the grace's head, a run");
    let gy = t.origin.y.0 + (t.bounds.bottom.0 + t.bounds.top.0) * 0.5;
    assert!(
        (gy - hy).abs() < 0.05,
        "at one height: grace {gy}, its note {hy}"
    );
    // a ledger line is a short stroke at a head's height or nearer the line: E4 stands two spaces under the line, as its note stands, and draws none by the five-line frame
    let s = l.systems().next().unwrap();
    let short = s
        .primitives
        .strokes
        .iter()
        .filter(|&&i| {
            let t = &l.strokes[i as usize];
            t.from.y == t.to.y
                && (t.to.x.0 - t.from.x.0).abs() < 3.0
                && (t.to.x.0 - t.from.x.0).abs() > 0.01
                && !matches!(t.provenance.source, TypedObjectId::Staff(_))
        })
        .count();
    assert_eq!(short, 0, "no ledger line by the one line's frame");
}

/// A rest filling its measure centres between its barlines though another
/// voice's notes stand between them.
#[test]
fn a_measure_rest_centres_past_another_voices_notes() {
    let q = "<note><pitch><step>C</step><octave>4</octave></pitch><duration>1</duration><voice>2</voice><type>quarter</type><stem>down</stem></note>";
    let m2 = format!("<note><rest measure=\"yes\"/><duration>4</duration><voice>1</voice></note><backup><duration>4</duration></backup>{q}{q}{q}{q}");
    let u = format!("<measure number=\"1\">{}{}</measure><measure number=\"2\">{m2}</measure><measure number=\"3\">{}</measure>", head(), whole("B", 4), whole("B", 4));
    let l = engrave_on(
        "rest_past_notes.musicxml",
        &score(&[u], None),
        PageGeometry::default(),
    );
    let r = l
        .glyphs
        .iter()
        .find(|g| g.glyph.as_str().starts_with("restWhole"))
        .unwrap();
    let mid = r.position.x.0 + (r.bounding_box.left.0 + r.bounding_box.right.0) * 0.5;
    let mut b: Vec<(f32, f32)> = l
        .glyphs
        .iter()
        .filter(|g| g.glyph.as_str() == "barlineSingle")
        .map(|g| {
            (
                g.position.x.0 + g.bounding_box.left.0,
                g.position.x.0 + g.bounding_box.right.0,
            )
        })
        .collect();
    b.sort_by(|a, c| a.0.total_cmp(&c.0));
    let want = (b[0].1 + b[1].0) * 0.5;
    assert!(
        (mid - want).abs() < 0.02,
        "the rest at the bar's middle: {mid} against {want}"
    );
}

/// The first system's bottom line to the second's top line, on a page tall
/// enough that nothing spreads or breaks, of a staff of two systems.
fn system_distance(placed: &epiphany_layout_ir::ConstrainedLayoutIR) -> f32 {
    let mut g = PageGeometry::default();
    g.size.height = StaffSpace(0.0);
    let layout = Engraver::with_geometry(g)
        .solve(placed, &SolverConfig::default())
        .layout;
    let v: Vec<_> = layout.systems().collect();
    assert_eq!(v.len(), 2, "two systems");
    lines(&layout, v[0])[0].0 - lines(&layout, v[1])[0].1
}

fn two_systems(name: &str) -> epiphany_layout_ir::ConstrainedLayoutIR {
    let u = format!(
        "<measure number=\"1\">{}{}</measure><measure number=\"2\"><print new-system=\"yes\"/>{}</measure>",
        head(),
        whole("B", 4),
        whole("B", 4)
    );
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::write(&path, score(&[u], None)).expect("written");
    let loaded = load(&path).expect("loads");
    to_constrained(&to_logical(&loaded.reduced.score))
}

/// The gap between two systems reads the ink of no staff. The importer draws
/// none but a traced anchor, so the constrained input is amended: the region's
/// anchor drawn as a line twelve spaces under the first system's staff.
#[test]
fn the_gap_between_systems_reads_the_ink_of_no_staff() {
    use epiphany_layout_ir::{Point, VerticalBandKind};
    let placed = two_systems("free_ink_between_systems.musicxml");
    let plain = system_distance(&placed);
    let margin: Vec<_> = placed
        .vertical_bands
        .iter()
        .filter(|b| b.kind == VerticalBandKind::MarginBand)
        .map(|b| b.id)
        .collect();
    let free = (0..placed.strokes.len())
        .find(|&i| margin.contains(&placed.strokes[i].vertical_band))
        .expect("the region's anchor");
    let bottom_line = placed
        .strokes
        .iter()
        .filter(|s| matches!(s.provenance.source, TypedObjectId::Staff(_)) && s.from.y == s.to.y)
        .map(|s| s.from.y.0)
        .fold(f32::INFINITY, f32::min);
    let mut lowered = placed.clone();
    let anchor = &mut lowered.strokes[free];
    let (x, y) = (anchor.from.x.0, bottom_line - 12.0);
    anchor.from = Point::new(x, y);
    anchor.to = Point::new(x + 20.0, y);
    anchor.thickness = StaffSpace(0.2);
    let distance = system_distance(&lowered);
    assert!(
        distance >= 12.4 && distance > plain + 1.0,
        "the line under the system holds the next one off: {distance}, without it {plain}"
    );
}

/// And a member of the family on no staff, which the importer does not make:
/// amended, the fermata's mark and glyph set in the margin band and twelve
/// spaces under the staff.
#[test]
fn the_gap_between_systems_reads_a_family_mark_of_no_staff() {
    use epiphany_layout_ir::{GlyphObjectId, Point, VerticalBandKind};
    let u = format!(
        "<measure number=\"1\">{}<note><pitch><step>B</step><octave>4</octave></pitch><duration>4</duration><voice>1</voice><type>whole</type><notations><fermata type=\"upright\"/></notations></note></measure><measure number=\"2\"><print new-system=\"yes\"/>{}</measure>",
        head(),
        whole("B", 4)
    );
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("free_family_between_systems.musicxml");
    std::fs::write(&path, score(&[u], None)).expect("written");
    let loaded = load(&path).expect("loads");
    let placed = to_constrained(&to_logical(&loaded.reduced.score));
    let plain = system_distance(&placed);
    let margin = placed
        .vertical_bands
        .iter()
        .find(|b| b.kind == VerticalBandKind::MarginBand)
        .expect("the margin band")
        .id;
    let bottom_line = placed
        .strokes
        .iter()
        .filter(|s| matches!(s.provenance.source, TypedObjectId::Staff(_)) && s.from.y == s.to.y)
        .map(|s| s.from.y.0)
        .fold(f32::INFINITY, f32::min);
    let mut lowered = placed.clone();
    let fermata: GlyphObjectId = lowered
        .glyphs
        .iter()
        .find(|g| g.glyph.as_str().starts_with("fermata"))
        .expect("the fermata")
        .id();
    let mark = lowered
        .family
        .iter_mut()
        .find(|m| m.glyphs.contains(&fermata))
        .expect("its member of the family");
    mark.band = margin;
    let glyph = lowered
        .glyphs
        .iter_mut()
        .find(|g| g.id() == fermata)
        .expect("the fermata");
    glyph.vertical_band = margin;
    glyph.baseline = Point::new(glyph.baseline.x.0, bottom_line - 12.0);
    for band in &mut lowered.vertical_bands {
        band.members.retain(|m| *m != fermata);
        if band.id == margin {
            band.members.push(fermata);
        }
    }
    let distance = system_distance(&lowered);
    assert!(
        distance >= 12.0 && distance > plain + 1.0,
        "the mark under the system holds the next one off: {distance}, without it {plain}"
    );
}

/// Only its own system's lead bounds a centred dynamic: on a page that spreads
/// nothing, in a gap a low note and a high one hold 5.5 wide, a treble clef's
/// reach on the system before leaves a wide dynamic under a bass clef where it
/// stands with bass clefs on every system.
#[test]
fn a_centred_dynamic_reads_only_its_own_systems_lead() {
    let staff_one = |treble: bool| {
        if treble {
            "<clef number=\"1\"><sign>G</sign><line>2</line></clef>"
        } else {
            "<clef number=\"1\"><sign>F</sign><line>4</line></clef>"
        }
    };
    let xml = |treble: bool| {
        let attrs = format!(
            "<attributes><divisions>1</divisions><time><beats>4</beats><beat-type>4</beat-type></time><staves>2</staves>{}<clef number=\"2\"><sign>F</sign><line>4</line></clef></attributes>",
            staff_one(treble)
        );
        let bass =
            "<attributes><clef number=\"1\"><sign>F</sign><line>4</line></clef></attributes>";
        let wide = "<direction><direction-type><dynamics><ffff/></dynamics></direction-type><staff>1</staff></direction>";
        let plain = format!(
            "{}{BK}{}",
            gn("C", 3, 4, "whole", 1, 1, None),
            gn("C", 3, 4, "whole", 5, 2, None)
        );
        let held = format!(
            "{wide}{}{}{BK}{}{}",
            gn("G", 3, 2, "half", 1, 1, None),
            gn("E", 2, 2, "half", 1, 1, None),
            gn("C", 3, 2, "half", 5, 2, None),
            gn("G", 4, 2, "half", 5, 2, None),
        );
        let change = if treble { bass } else { "" };
        format!(
            "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\"><part-name>A</part-name></score-part></part-list><part id=\"P1\"><measure number=\"1\">{attrs}{plain}</measure><measure number=\"2\"><print new-system=\"yes\"/>{plain}</measure><measure number=\"3\"><print new-system=\"yes\"/>{change}{held}</measure></part></score-partwise>"
        )
    };
    let under = |name: &str, treble: bool| {
        let mut g = PageGeometry::default();
        g.size.height = StaffSpace(0.0);
        let layout = engrave_on(name, &xml(treble), g);
        let f = layout
            .glyphs
            .iter()
            .find(|g| g.glyph.as_str().starts_with("dynamicF"))
            .expect("the dynamic");
        let system = layout.systems().nth(2).expect("a third system");
        let v = lines(&layout, system);
        (v[0].0 - f.position.y.0, v[0].0 - v[1].1)
    };
    let ((a, gap_a), (b, gap_b)) = (
        under("centred_lead_treble.musicxml", true),
        under("centred_lead_bass.musicxml", false),
    );
    assert!(
        (gap_a - gap_b).abs() < 0.01 && gap_a > 4.8,
        "one gap: {gap_a} and {gap_b}"
    );
    assert!(
        (a - b).abs() < 0.01,
        "under its staff by {a} after a treble clef's system, by {b} after a bass clef's, in a gap of {gap_a}"
    );
}

/// Each dotted rest's dot, by how far it stands above its rest's own height.
fn dots_over_rests(l: &ResolvedLayoutIR) -> Vec<f32> {
    l.glyphs
        .iter()
        .filter(|g| g.glyph.as_str().starts_with("rest"))
        .filter_map(|r| {
            let right = r.position.x.0 + r.bounding_box.right.0;
            l.glyphs
                .iter()
                .filter(|d| d.glyph.as_str() == "augmentationDot")
                .filter(|d| d.position.x.0 >= right && d.position.x.0 < right + 1.5)
                .map(|d| d.position.y.0 - r.position.y.0)
                .next()
        })
        .collect()
}

/// A dotted half rest and a quarter rest in one voice, whole-note neighbours
/// around it, beside two halves of another voice.
fn dotted_rest_beside(voice: u8, other: u8, step: &str, octave: u8) -> String {
    let rest = |d: u8, kind: &str, dot: &str| {
        format!("<note><rest/><duration>{d}</duration><voice>{voice}</voice><type>{kind}</type>{dot}</note>")
    };
    let half = format!("<note><pitch><step>{step}</step><octave>{octave}</octave></pitch><duration>2</duration><voice>{other}</voice><type>half</type></note>");
    let bar = format!(
        "{}{}<backup><duration>4</duration></backup>{half}{half}",
        rest(3, "half", "<dot/>"),
        rest(1, "quarter", "")
    );
    score(
        &[format!(
            "<measure number=\"1\">{}{}</measure><measure number=\"2\">{bar}</measure><measure number=\"3\">{}</measure>",
            head(),
            whole("C", 5),
            whole("C", 5)
        )],
        None,
    )
}

/// A dot keeps its place against its rest wherever the rest stands (Gould,
/// p. 38): half a space over a half rest, as on a staff of one voice, where the
/// rest stands aside for its voice and nothing near it moves it further.
#[test]
fn a_voices_dotted_rest_keeps_its_dot_beside_it() {
    let upper = engrave_on(
        "dot_upper.musicxml",
        &dotted_rest_beside(1, 2, "E", 4),
        PageGeometry::default(),
    );
    assert_eq!(dots_over_rests(&upper), vec![0.5], "an upper voice's");
    let lower = engrave_on(
        "dot_lower.musicxml",
        &dotted_rest_beside(2, 1, "F", 5),
        PageGeometry::default(),
    );
    assert_eq!(dots_over_rests(&lower), vec![0.5], "a lower voice's");
}

/// And where the other voice's heads move the rest further, each way.
#[test]
fn a_dotted_rest_moved_off_a_voices_heads_takes_its_dot() {
    let upper = engrave_on(
        "dot_cleared_upper.musicxml",
        &dotted_rest_beside(1, 2, "C", 6),
        PageGeometry::default(),
    );
    assert_eq!(dots_over_rests(&upper), vec![0.5], "up, off a high note");
    let lower = engrave_on(
        "dot_cleared_lower.musicxml",
        &dotted_rest_beside(2, 1, "A", 3),
        PageGeometry::default(),
    );
    assert_eq!(dots_over_rests(&lower), vec![0.5], "down, off a low note");
}

/// A dotted half rest over a whole note's flat: the ledger line's overhang
/// reaches the flat where the plain glyph does not, and the move off it takes
/// the dot along, as every earlier move has.
#[test]
fn a_ledgered_rests_dot_moves_with_it_off_an_accidental() {
    let bar = "<note><rest/><duration>3</duration><voice>1</voice><type>half</type><dot/></note>\
        <note><pitch><step>A</step><octave>5</octave></pitch><duration>1</duration><voice>1</voice><type>quarter</type></note>\
        <backup><duration>4</duration></backup>\
        <note><pitch><step>G</step><alter>-1</alter><octave>5</octave></pitch><duration>4</duration><voice>2</voice><type>whole</type><accidental>flat</accidental></note>";
    let xml = score(
        &[format!(
            "<measure number=\"1\">{}{}</measure><measure number=\"2\">{bar}</measure><measure number=\"3\">{}</measure>",
            head(),
            whole("C", 5),
            whole("C", 5)
        )],
        None,
    );
    let l = engrave_on("dot_ledger.musicxml", &xml, PageGeometry::default());
    assert!(
        l.glyphs
            .iter()
            .any(|g| g.glyph.as_str() == "restHalfLegerLine"),
        "it stands ledgered"
    );
    assert_eq!(dots_over_rests(&l), vec![0.5]);
}

/// Clauses of the rest filling its measure and what it carries, each on a score
/// whose page reads only that clause.
mod rest_clauses {
    use super::*;

    fn engrave(name: &str, xml: &str) -> ResolvedLayoutIR {
        engrave_on(name, xml, PageGeometry::default())
    }

    fn one_part(measures: &str) -> String {
        score(&[measures.to_owned()], None)
    }

    fn two_parts(a: &str, b: &str) -> String {
        score(&[a.to_owned(), b.to_owned()], None)
    }

    fn attrs(beats: u32, beat_type: u32) -> String {
        format!("<attributes><divisions>4</divisions><time><beats>{beats}</beats><beat-type>{beat_type}</beat-type></time>{TREBLE}</attributes>")
    }

    fn voiced(step: &str, octave: u32, d: u32, kind: &str, voice: u32, extra: &str) -> String {
        format!("<note><pitch><step>{step}</step><octave>{octave}</octave></pitch><duration>{d}</duration><voice>{voice}</voice><type>{kind}</type>{extra}</note>")
    }

    fn rest(d: u32, voice: u32, extra: &str) -> String {
        format!("<note><rest measure=\"yes\"/><duration>{d}</duration><voice>{voice}</voice>{extra}</note>")
    }

    fn hidden_rest(d: u32, voice: u32, extra: &str) -> String {
        format!("<note print-object=\"no\"><rest measure=\"yes\"/><duration>{d}</duration><voice>{voice}</voice>{extra}</note>")
    }

    fn back(d: u32) -> String {
        format!("<backup><duration>{d}</duration></backup>")
    }

    fn slur(kind: &str) -> String {
        format!("<notations><slur type=\"{kind}\" number=\"1\"/></notations>")
    }

    fn c5() -> String {
        voiced("C", 5, 16, "whole", 1, "")
    }

    fn quarters() -> String {
        ["C", "D", "E", "F"]
            .iter()
            .map(|p| voiced(p, 5, 4, "quarter", 1, ""))
            .collect()
    }

    /// Three bars of quarters, each staff of the system kept wide by them.
    fn quarter_part() -> String {
        format!(
            "<measure number=\"1\">{}{q}</measure><measure number=\"2\">{q}</measure><measure number=\"3\">{q}</measure>",
            attrs(4, 4),
            q = quarters()
        )
    }

    fn barlines(l: &ResolvedLayoutIR) -> Vec<f32> {
        let mut bars: Vec<f32> = l
            .glyphs
            .iter()
            .filter(|g| g.glyph.as_str() == "barlineSingle")
            .map(|g| g.position.x.0)
            .collect();
        bars.sort_by(f32::total_cmp);
        bars.dedup_by(|a, b| (*a - *b).abs() < 0.01);
        bars
    }

    fn rests_of(l: &ResolvedLayoutIR) -> Vec<&epiphany_layout_ir::ResolvedGlyph> {
        l.glyphs
            .iter()
            .filter(|g| g.glyph.as_str().starts_with("rest"))
            .collect()
    }

    /// A tie is read at its true arc and thickness: a B5 held under an upper
    /// voice's rest takes it to the first height clear of the arc, not its chord.
    #[test]
    fn a_rest_reads_a_ties_true_arc() {
        let held = |t: &str| {
            format!("<note><pitch><step>B</step><octave>5</octave></pitch><duration>16</duration><tie type=\"{t}\"/><voice>2</voice><type>whole</type><stem>down</stem><notations><tied type=\"{t}\"/></notations></note>")
        };
        let xml = one_part(&format!(
            "<measure number=\"1\">{}{}</measure><measure number=\"2\">{}{}{}</measure><measure number=\"3\">{}{}{}</measure><measure number=\"4\">{}</measure>",
            attrs(4, 4),
            c5(),
            rest(16, 1, ""),
            back(16),
            held("start"),
            rest(16, 1, ""),
            back(16),
            held("stop"),
            c5()
        ));
        let l = engrave("rest_tie_arc.musicxml", &xml);
        let top = lines(&l, l.systems().next().expect("a system"))[0].1;
        let ys: Vec<f32> = rests_of(&l).iter().map(|g| g.position.y.0 - top).collect();
        assert!(
            (ys[0] - 2.0).abs() < 0.01,
            "the first rest over the arc: {ys:?}"
        );
    }

    /// A bar of 1/64 between bars holding a high note: such ink, of the bar before,
    /// stands within a rest's reach and moves it no further than its neighbours.
    #[test]
    fn a_measure_rest_reads_no_ink_of_the_bar_before() {
        let mut m = String::new();
        for k in 0..24 {
            let body = if k % 2 == 1 {
                format!(
                    "{}{}{}",
                    rest(1, 1, ""),
                    back(1),
                    voiced("E", 4, 1, "64th", 2, "<stem>down</stem>")
                )
            } else if k % 4 == 0 {
                voiced("A", 6, 1, "64th", 1, "<stem>up</stem>")
            } else {
                voiced("E", 4, 1, "64th", 1, "<stem>up</stem>")
            };
            let at = if k == 0 {
                format!("<attributes><divisions>16</divisions><time><beats>1</beats><beat-type>64</beat-type></time>{TREBLE}</attributes>")
            } else {
                String::new()
            };
            m += &format!("<measure number=\"{}\">{at}{body}</measure>", k + 1);
        }
        let l = engrave("rest_bar_before.musicxml", &one_part(&m));
        let ys: Vec<f32> = l
            .systems()
            .flat_map(|sys| {
                let top = lines(&l, sys)[0].1;
                sys.primitives
                    .glyphs
                    .iter()
                    .map(|&i| &l.glyphs[i as usize])
                    .filter(|g| g.glyph.as_str().starts_with("rest"))
                    .map(move |g| g.position.y.0 - top)
                    .collect::<Vec<_>>()
            })
            .collect();
        let lo = ys.iter().copied().fold(f32::INFINITY, f32::min);
        for y in &ys {
            assert!((y - lo).abs() < 0.01, "moved by the bar before: {ys:?}");
        }
    }

    /// And of the bar after: the note opening it.
    #[test]
    fn a_measure_rest_reads_no_ink_of_the_bar_after() {
        let mut m = String::new();
        for k in 0..16 {
            let body = if k % 2 == 0 {
                format!(
                    "{}{}{}",
                    rest(1, 1, ""),
                    back(1),
                    voiced("E", 4, 1, "16th", 2, "<stem>down</stem>")
                )
            } else {
                format!(
                    "{}{}{}",
                    voiced("A", 5, 1, "16th", 1, "<stem>down</stem>"),
                    back(1),
                    voiced("E", 4, 1, "16th", 2, "<stem>down</stem>")
                )
            };
            let at = if k == 0 { attrs(1, 16) } else { String::new() };
            m += &format!("<measure number=\"{}\">{at}{body}</measure>", k + 1);
        }
        let l = engrave("rest_bar_after.musicxml", &one_part(&m));
        for g in rests_of(&l) {
            assert_eq!(g.glyph.as_str(), "restWhole", "at {:?}", g.position);
        }
    }

    /// A slur's carry moves its own end alone: one starting on a rest ends where its
    /// note stands, and one ending on a rest starts at its note.
    #[test]
    fn a_slur_from_a_rest_keeps_its_other_end_at_its_note() {
        let stands_at_a_note = |l: &ResolvedLayoutIR, end: f32, what: &str| {
            let near = l
                .glyphs
                .iter()
                .filter(|g| g.glyph.as_str() == "noteheadWhole")
                .map(|g| (end - g.position.x.0).abs())
                .fold(f32::INFINITY, f32::min);
            assert!(near < 2.5, "{what} {end}, {near} from a note");
        };
        let four = |m2: String, m3: String| {
            one_part(&format!(
                "<measure number=\"1\">{}{}</measure><measure number=\"2\">{m2}</measure><measure number=\"3\">{m3}</measure><measure number=\"4\">{}</measure>",
                attrs(4, 4),
                c5(),
                c5()
            ))
        };
        let started = engrave(
            "slur_from_rest.musicxml",
            &four(
                rest(16, 1, &slur("start")),
                voiced("D", 5, 16, "whole", 1, &slur("stop")),
            ),
        );
        let c = started.curves[0].control_points();
        stands_at_a_note(&started, c[3].x.0, "its end");
        let ended = engrave(
            "slur_to_rest.musicxml",
            &four(
                voiced("D", 5, 16, "whole", 1, &slur("start")),
                rest(16, 1, &slur("stop")),
            ),
        );
        let c = ended.curves[0].control_points();
        stands_at_a_note(&ended, c[0].x.0, "its start");
    }

    /// A pedal ended earlier in the bar is not across a dynamic centred after it: the
    /// p stays about the gap's middle.
    #[test]
    fn a_pedal_earlier_in_the_bar_leaves_a_dynamic_centred() {
        let upper = |p: &str, o: u32| {
            format!("<note><pitch><step>{p}</step><octave>{o}</octave></pitch><duration>1</duration><voice>1</voice><type>quarter</type><staff>1</staff></note>")
        };
        let lower = |p: &str, o: u32| {
            format!("<note><pitch><step>{p}</step><octave>{o}</octave></pitch><duration>1</duration><voice>5</voice><type>quarter</type><staff>2</staff></note>")
        };
        let ped = |k: &str| {
            format!("<direction><direction-type><pedal type=\"{k}\" line=\"yes\"/></direction-type><staff>1</staff></direction>")
        };
        let p = "<direction><direction-type><dynamics><p/></dynamics></direction-type><staff>1</staff></direction>";
        let head = "<attributes><divisions>1</divisions><time><beats>4</beats><beat-type>4</beat-type></time><staves>2</staves><clef number=\"1\"><sign>G</sign><line>2</line></clef><clef number=\"2\"><sign>F</sign><line>4</line></clef></attributes>";
        let up = format!(
            "{}{}{}{}{}{p}{}",
            ped("start"),
            upper("A", 4),
            upper("A", 4),
            ped("stop"),
            upper("A", 4),
            upper("A", 4)
        );
        let down = lower("C", 3).repeat(4);
        let wide = format!(
            "{}{}{}",
            upper("A", 5).repeat(4),
            "<backup><duration>4</duration></backup>",
            lower("A", 2).repeat(4)
        );
        let xml = one_part(&format!(
            "<measure number=\"1\">{head}{up}<backup><duration>4</duration></backup>{down}</measure><measure number=\"2\">{wide}</measure>"
        ));
        let l = engrave("pedal_before_dynamic.musicxml", &xml);
        let d = l
            .glyphs
            .iter()
            .find(|g| g.glyph.as_str() == "dynamicPiano")
            .expect("the p");
        let staves = lines(&l, l.systems().next().expect("a system"));
        let middle = (staves[0].0 + staves[1].1) * 0.5;
        let centre = d.position.y.0 + (d.bounding_box.bottom.0 + d.bounding_box.top.0) * 0.5;
        assert!(
            (centre - middle).abs() < 1.0,
            "the p about the gap's middle: {centre} against {middle}"
        );
    }

    /// A slur on a hidden rest, in a bar another staff gives width, is carried as on a
    /// drawn one: a hidden rest is centred for what stands on it.
    #[test]
    fn a_slur_on_a_hidden_rest_starts_at_the_bars_middle() {
        let a = format!(
            "<measure number=\"1\">{}{}</measure><measure number=\"2\">{}</measure><measure number=\"3\">{}</measure>",
            attrs(4, 4),
            c5(),
            hidden_rest(16, 1, &slur("start")),
            voiced("D", 5, 16, "whole", 1, &slur("stop"))
        );
        let l = engrave("slur_hidden_rest.musicxml", &two_parts(&a, &quarter_part()));
        let c = l.curves[0].control_points();
        let bars = barlines(&l);
        let middle = (bars[0] + bars[1]) * 0.5;
        assert!(
            (c[0].x.0 - middle).abs() < 0.5,
            "the slur's start {} at the bar's middle {middle}",
            c[0].x.0
        );
    }

    /// A syllable on a hidden rest stays at its column: it has no glyph to ride.
    #[test]
    fn a_syllable_on_a_hidden_rest_stays_at_its_column() {
        let lyric = "<lyric number=\"1\"><syllabic>single</syllabic><text>two</text></lyric>";
        let a = format!(
            "<measure number=\"1\">{}{}</measure><measure number=\"2\">{}</measure><measure number=\"3\">{}</measure>",
            attrs(4, 4),
            c5(),
            hidden_rest(16, 1, lyric),
            voiced("D", 5, 16, "whole", 1, "")
        );
        let l = engrave(
            "syllable_hidden_rest.musicxml",
            &two_parts(&a, &quarter_part()),
        );
        let t = l
            .texts
            .iter()
            .find(|t| t.text.as_str() == "two")
            .expect("the syllable");
        let x = t.origin.x.0 + (t.bounds.left.0 + t.bounds.right.0) * 0.5;
        let bars = barlines(&l);
        assert!(
            x > bars[0] && x < bars[0] + 6.0,
            "the syllable {x} at its column after {}",
            bars[0]
        );
    }

    fn ottava(kind: &str) -> String {
        let place = if kind == "stop" {
            ""
        } else {
            " placement=\"above\""
        };
        format!("<direction{place}><direction-type><octave-shift type=\"{kind}\" size=\"8\" number=\"1\"/></direction-type></direction>")
    }

    fn ottava_hook(l: &ResolvedLayoutIR) -> f32 {
        l.strokes
            .iter()
            .filter(|s| {
                matches!(s.provenance.source, TypedObjectId::Spanner(_))
                    && s.from.x == s.to.x
                    && s.from.y != s.to.y
            })
            .map(|s| s.from.x.0)
            .fold(f32::NEG_INFINITY, f32::max)
    }

    /// An ottava's end on a rest reads its own staff's rest, not the other part's,
    /// whose clef closing the bar stands it elsewhere, either part above.
    #[test]
    fn an_ottava_reads_the_rest_of_its_own_staff() {
        let clef = "<attributes><clef><sign>F</sign><line>4</line></clef></attributes>";
        let with_ottava = format!(
            "<measure number=\"1\">{}{}{}</measure><measure number=\"2\">{}{}</measure><measure number=\"3\">{}</measure>",
            attrs(4, 4),
            ottava("down"),
            voiced("C", 6, 16, "whole", 1, ""),
            rest(16, 1, ""),
            ottava("stop"),
            c5()
        );
        let other = format!(
            "<measure number=\"1\">{}{}</measure><measure number=\"2\">{}{clef}</measure><measure number=\"3\">{}</measure>",
            attrs(4, 4),
            c5(),
            rest(16, 1, ""),
            voiced("C", 3, 16, "whole", 1, "")
        );
        for (name, xml) in [
            ("ottava_own_above.musicxml", two_parts(&with_ottava, &other)),
            ("ottava_own_below.musicxml", two_parts(&other, &with_ottava)),
        ] {
            let l = engrave(name, &xml);
            let hook = ottava_hook(&l);
            let right = rests_of(&l)
                .iter()
                .map(|g| g.position.x.0 + g.bounding_box.right.0)
                .fold(f32::NEG_INFINITY, f32::max);
            assert!(
                (hook - (right + 1.0)).abs() < 0.2,
                "{name}: the hook {hook} a space past its own rest {right}"
            );
        }
    }

    /// And the drawn rest beside a hidden one in its column, either voice hidden.
    #[test]
    fn an_ottava_reads_the_drawn_rest_beside_a_hidden_one() {
        for (name, bar) in [
            (
                "ottava_hidden_first.musicxml",
                format!(
                    "{}{}{}{}",
                    hidden_rest(16, 1, ""),
                    back(16),
                    rest(16, 2, ""),
                    ottava("stop")
                ),
            ),
            (
                "ottava_hidden_second.musicxml",
                format!(
                    "{}{}{}{}",
                    rest(16, 1, ""),
                    back(16),
                    hidden_rest(16, 2, ""),
                    ottava("stop")
                ),
            ),
        ] {
            let m = format!(
                "<measure number=\"1\">{}{}{}</measure><measure number=\"2\">{bar}</measure><measure number=\"3\">{}</measure>",
                attrs(4, 4),
                ottava("down"),
                voiced("C", 6, 16, "whole", 1, ""),
                c5()
            );
            let l = engrave(name, &one_part(&m));
            let hook = ottava_hook(&l);
            let g = rests_of(&l)[0];
            let right = g.position.x.0 + g.bounding_box.right.0;
            assert!(
                (hook - (right + 1.0)).abs() < 0.2,
                "{name}: the hook {hook} past the drawn rest {right}"
            );
        }
    }

    /// A measure rest's stems hold its own bar's visitors alone, read as the
    /// constrained stage gives them: six bars, the voice visiting in two, the bar
    /// before the rest's among them.
    #[test]
    fn a_measure_rests_stems_hold_only_its_own_bar() {
        let clefs = "<staves>3</staves><clef number=\"1\"><sign>G</sign><line>2</line></clef><clef number=\"2\"><sign>G</sign><line>2</line></clef><clef number=\"3\"><sign>F</sign><line>4</line></clef>";
        let attributes = format!("<attributes><divisions>4</divisions><time><beats>1</beats><beat-type>8</beat-type></time>{clefs}</attributes>");
        let on = |p: &str, o: u32, v: u32, st: u32| {
            format!("<note><pitch><step>{p}</step><octave>{o}</octave></pitch><duration>2</duration><voice>{v}</voice><type>eighth</type><staff>{st}</staff></note><backup><duration>2</duration></backup>")
        };
        let plain = format!(
            "{}{}{}",
            on("C", 5, 1, 1),
            on("B", 4, 5, 2),
            on("C", 3, 9, 3)
        );
        let before = format!(
            "{}{}{}",
            on("C", 5, 1, 1),
            on("B", 4, 5, 2),
            on("F", 5, 9, 2)
        );
        let own = format!(
            "{}<note><rest measure=\"yes\"/><duration>2</duration><voice>5</voice><staff>2</staff></note><backup><duration>2</duration></backup>{}",
            on("C", 5, 1, 1),
            on("C", 4, 9, 2)
        );
        let xml = one_part(&format!(
            "<measure number=\"1\">{attributes}{plain}</measure><measure number=\"2\">{plain}</measure><measure number=\"3\">{before}</measure><measure number=\"4\">{own}</measure><measure number=\"5\">{plain}</measure><measure number=\"6\">{plain}</measure>"
        ));
        let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("rest_stems_own_bar.musicxml");
        std::fs::write(&path, xml).expect("written");
        let loaded = load(&path).expect("loads");
        let constrained = to_constrained(&to_logical(&loaded.reduced.score));
        let rests = &constrained.measure_rests;
        assert_eq!(rests.len(), 1);
        assert!(!rests[0].stems.is_empty(), "its bar's visitor is read");
        assert!(
            rests[0]
                .stems
                .iter()
                .all(|(slot, ..)| *slot == rests[0].slot),
            "{} stems, some of another bar",
            rests[0].stems.len()
        );
    }
}
