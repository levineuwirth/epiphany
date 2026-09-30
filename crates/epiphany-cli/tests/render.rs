//! The `epiphany` command on the importer's hand-written fixtures, and the
//! page a render keeps.

use std::path::{Path, PathBuf};
use std::process::Command;

use epiphany_cli::{engrave, load, page};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../epiphany-musicxml/tests/fixtures")
        .join(name)
}

#[test]
fn render_writes_the_requested_page_and_refuses_one_past_the_end() {
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("single_part.svg");
    let _ = std::fs::remove_file(&out);
    let status = Command::new(env!("CARGO_BIN_EXE_epiphany"))
        .arg("render")
        .arg(fixture("single_part.musicxml"))
        .args(["--page", "1", "-o"])
        .arg(&out)
        .output()
        .expect("runs");
    assert!(status.status.success(), "{status:?}");
    let svg = std::fs::read_to_string(&out).expect("an SVG was written");
    assert!(svg.starts_with("<?xml") && svg.contains("<svg"));
    assert!(svg.contains("noteheadBlack"), "the page carries the notes");

    let past = Command::new(env!("CARGO_BIN_EXE_epiphany"))
        .arg("render")
        .arg(fixture("single_part.musicxml"))
        .args(["--page", "2"])
        .output()
        .expect("runs");
    assert_eq!(past.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&past.stderr).contains("page 2 of 1"));
}

#[test]
fn import_exits_nonzero_on_a_named_refusal() {
    let out = Command::new(env!("CARGO_BIN_EXE_epiphany"))
        .arg("import")
        .arg(fixture("timewise.musicxml"))
        .output()
        .expect("runs");
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("not <score-partwise>"));
}

/// A generated score long enough to break onto several pages.
fn long_score() -> String {
    let mut measures = String::new();
    for m in 1..=320 {
        measures.push_str(&format!("<measure number=\"{m}\">"));
        if m == 1 {
            measures.push_str(
                "<attributes><divisions>1</divisions><time><beats>4</beats>\
                 <beat-type>4</beat-type></time><clef><sign>G</sign><line>2</line></clef>\
                 </attributes>",
            );
        }
        for step in ["C", "D", "E", "F"] {
            measures.push_str(&format!(
                "<note><pitch><step>{step}</step><octave>5</octave></pitch>\
                 <duration>1</duration><voice>1</voice></note>"
            ));
        }
        measures.push_str("</measure>");
    }
    format!(
        "<score-partwise version=\"4.0\"><part-list><score-part id=\"P1\">\
         <part-name>Flute</part-name></score-part></part-list>\
         <part id=\"P1\">{measures}</part></score-partwise>"
    )
}

#[test]
fn a_page_keeps_exactly_the_primitives_its_systems_own() {
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("long_score.musicxml");
    std::fs::write(&path, long_score()).expect("written");
    let loaded = load(&path).expect("loads");
    let layout = engrave(&loaded.reduced.score).layout;
    assert!(
        layout.pages.len() > 1,
        "the score breaks onto several pages"
    );
    let owned: usize = layout.systems().map(|s| s.primitives.glyphs.len()).sum();
    let mut total = 0;
    for number in 1..=layout.pages.len() {
        let one = page(&layout, number).expect("the page exists");
        let expected: usize = layout.pages[number - 1]
            .systems
            .iter()
            .map(|s| s.primitives.glyphs.len())
            .sum();
        assert_eq!(one.glyphs.len(), expected, "page {number}");
        assert_eq!(one.pages.len(), 1);
        total += one.glyphs.len();
    }
    assert_eq!(total, owned, "every owned glyph is on exactly one page");
    assert!(page(&layout, layout.pages.len() + 1).is_none());
    assert!(page(&layout, 0).is_none());
}
