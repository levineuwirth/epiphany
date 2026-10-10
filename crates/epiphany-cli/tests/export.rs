//! Export: every page as an SVG framed by its page rectangle, and all of them
//! as one PDF.

use std::path::{Path, PathBuf};

use epiphany_cli::{engrave_path, export, new_document, ExportOptions};

/// A one-part score of `measures` whole notes whose file sets a page small
/// enough to need several: 600 by 200 tenths with 50-tenth margins (60 by 20
/// staff spaces, room for one system), at 4 mm to forty tenths (1 mm to the
/// staff space).
fn paged(measures: usize) -> String {
    let mut body = String::new();
    for m in 1..=measures {
        body.push_str(&format!("<measure number=\"{m}\">"));
        if m == 1 {
            body.push_str(
                "<attributes><divisions>1</divisions><time><beats>4</beats>\
                 <beat-type>4</beat-type></time><clef><sign>G</sign><line>2</line></clef>\
                 </attributes>",
            );
        }
        body.push_str(
            "<note><pitch><step>E</step><octave>4</octave></pitch><duration>4</duration>\
             <type>whole</type></note></measure>",
        );
    }
    format!(
        "<score-partwise version=\"4.0\"><defaults><scaling><millimeters>4</millimeters>\
         <tenths>40</tenths></scaling><page-layout><page-width>600</page-width>\
         <page-height>200</page-height><page-margins type=\"both\"><left-margin>50</left-margin>\
         <right-margin>50</right-margin><top-margin>50</top-margin>\
         <bottom-margin>50</bottom-margin></page-margins></page-layout></defaults>\
         <part-list><score-part id=\"P1\"><part-name>A</part-name></score-part></part-list>\
         <part id=\"P1\">{body}</part></score-partwise>"
    )
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("export-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The PDF's page boxes, as written.
fn media_boxes(pdf: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(pdf);
    text.match_indices("/MediaBox [")
        .map(|(at, _)| {
            let rest = &text[at + "/MediaBox [".len()..];
            rest[..rest.find(']').unwrap()].to_owned()
        })
        .collect()
}

#[test]
fn every_page_exports_as_a_framed_svg_and_into_one_pdf() {
    let dir = scratch("pages");
    let source = dir.join("paged.musicxml");
    std::fs::write(&source, paged(24)).unwrap();
    let engraved = engrave_path(&source).expect("the score engraves");
    let pages = engraved.layout.pages.len();
    assert!(pages > 1, "the small page needs several, got {pages}");

    let exported = export(&engraved, &dir.join("out"), &ExportOptions::default()).unwrap();
    assert!(exported.overruns.is_empty(), "one staff fits its page");
    let written = exported.paths;
    assert_eq!(written.len(), pages + 1, "an SVG per page and the PDF");
    for (index, path) in written[..pages].iter().enumerate() {
        assert_eq!(
            path.file_name().unwrap().to_string_lossy(),
            format!("out-{}.svg", index + 1)
        );
        let svg = std::fs::read_to_string(path).unwrap();
        assert!(
            svg.contains("viewBox=\"0 0 60 20\""),
            "page {} is framed by its 60-by-20 page",
            index + 1
        );
        // The frame is the page's own: its content is translated by the page's
        // place in the stacked world, not cropped to its ink.
        let top = -(index as f32) * (20.0 + epiphany_engrave::INTER_PAGE_GAP);
        assert!(
            svg.contains(&format!("<g transform=\"translate(0 {})", fmt(top))),
            "page {} maps its own frame onto the view",
            index + 1
        );
        assert!(svg.contains("<path"), "page {} draws glyphs", index + 1);
    }
    let pdf = std::fs::read(&written[pages]).unwrap();
    assert!(written[pages].ends_with("out.pdf"));
    // 60 by 20 staff spaces at the file's 1 mm to the staff space.
    let scale = 1.0 * 72.0 / 25.4;
    assert_eq!(
        media_boxes(&pdf),
        vec![format!("0 0 {} {}", fmt(60.0 * scale), fmt(20.0 * scale)); pages]
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_document_exports_at_the_scale_asked_or_two_millimeters() {
    let dir = scratch("document");
    let source = dir.join("paged.musicxml");
    std::fs::write(&source, paged(4)).unwrap();
    let target = dir.join("paged.musc");
    drop(new_document(&source, &target).unwrap());
    let engraved = engrave_path(&target).expect("the document engraves");
    assert_eq!(engraved.staff_space_mm, None, "a document holds no scale");
    let only_pdf = ExportOptions {
        svg: false,
        ..ExportOptions::default()
    };
    let written = export(&engraved, &dir.join("default"), &only_pdf)
        .unwrap()
        .paths;
    assert_eq!(written.len(), 1);
    let boxes = media_boxes(&std::fs::read(&written[0]).unwrap());
    assert_eq!(
        boxes[0],
        format!(
            "0 0 {} {}",
            fmt(60.0 * (2.0 * 72.0 / 25.4)),
            fmt(20.0 * (2.0 * 72.0 / 25.4))
        )
    );
    let asked = ExportOptions {
        staff_space_mm: Some(1.5),
        ..only_pdf
    };
    let written = export(&engraved, &dir.join("asked"), &asked).unwrap().paths;
    let boxes = media_boxes(&std::fs::read(&written[0]).unwrap());
    assert_eq!(
        boxes[0],
        format!(
            "0 0 {} {}",
            fmt(60.0 * (1.5 * 72.0 / 25.4)),
            fmt(20.0 * (1.5 * 72.0 / 25.4))
        )
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A score of `parts` one-staff parts on the small page of [`paged`]: a
/// system taller than the page.
fn tall(parts: usize) -> String {
    let mut list = String::new();
    let mut body = String::new();
    for p in 1..=parts {
        list.push_str(&format!(
            "<score-part id=\"P{p}\"><part-name>{p}</part-name></score-part>"
        ));
        body.push_str(&format!(
            "<part id=\"P{p}\"><measure number=\"1\"><attributes><divisions>1</divisions>\
             <time><beats>4</beats><beat-type>4</beat-type></time><clef><sign>G</sign>\
             <line>2</line></clef></attributes><note><pitch><step>E</step><octave>4</octave>\
             </pitch><duration>4</duration><type>whole</type></note></measure></part>"
        ));
    }
    let paged = paged(1);
    let defaults =
        &paged[paged.find("<defaults>").unwrap()..paged.find("</defaults>").unwrap() + 11];
    format!(
        "<score-partwise version=\"4.0\">{defaults}<part-list>{list}</part-list>{body}\
         </score-partwise>"
    )
}

/// A page whose system runs past its paper is drawn whole, in a frame extended
/// to hold it, and the overrun is reported rather than the music cut off.
#[test]
fn a_page_whose_music_overruns_its_paper_is_extended_not_cut() {
    let dir = scratch("tall");
    let source = dir.join("tall.musicxml");
    std::fs::write(&source, tall(6)).unwrap();
    let engraved = engrave_path(&source).expect("engraves");
    let exported = export(&engraved, &dir.join("tall"), &ExportOptions::default()).unwrap();
    assert_eq!(exported.overruns.len(), engraved.layout.pages.len());
    let (number, overrun) = exported.overruns[0];
    assert_eq!(number, 1);
    assert!(overrun > 0.0);
    let page = epiphany_cli::page(&engraved.layout, 1).unwrap();
    let ink = epiphany_render_svg::ink_frame(&page).unwrap();
    let svg = std::fs::read_to_string(&exported.paths[0]).unwrap();
    // The frame keeps the paper's width and top, and reaches down to the ink.
    let height = 20.0 + overrun;
    assert!(
        svg.contains(&format!("viewBox=\"0 0 60 {}\"", fmt(height))),
        "the frame grows to {height}: {}",
        &svg[..svg.find("<!--").unwrap()]
    );
    assert!(ink.bottom >= -height - 1e-3);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A number as the renderers write one: at most four decimals, no trailing
/// zeros.
fn fmt(v: f32) -> String {
    let v = if v == 0.0 { 0.0 } else { v };
    let mut s = format!("{v:.4}");
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    s
}
