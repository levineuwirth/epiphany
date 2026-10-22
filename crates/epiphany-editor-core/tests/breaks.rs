//! Line and page breaks set and cleared from the editor, and laid out.

use epiphany_bundle::MemStore;
use epiphany_core::{CmnNominal, MusicalDuration, MusicalPosition, RationalTime, ReplicaId};
use epiphany_core::{RegionId, TypedObjectId};
use epiphany_editor_core::{EditorDocument, EditorError, EditorSession, LayoutBreak, ScoreSetup};
use epiphany_engrave::Engraver;

fn at(n: i64, d: i64) -> MusicalPosition {
    MusicalPosition(RationalTime::new(n, d).expect("a valid position"))
}

/// Four measures of whole notes, C D E F, engraved by the real solver: one
/// system on one page.
fn four_measures() -> (EditorSession, RegionId) {
    let operations = ScoreSetup::single_staff("Flute", 4)
        .operations(ReplicaId::generate())
        .unwrap();
    let mut document = EditorDocument::create(MemStore::new(), operations).unwrap();
    let mut session = document.lease(Box::new(Engraver::default())).unwrap();
    let (region, _, voice) = session.score().voices().next().unwrap();
    let voice = voice.id;
    session
        .set_caret(
            voice,
            MusicalPosition::origin(),
            MusicalDuration(RationalTime::new(1, 1).unwrap()),
        )
        .unwrap();
    for nominal in [CmnNominal::C, CmnNominal::D, CmnNominal::E, CmnNominal::F] {
        session.enter_nominal(nominal).unwrap();
    }
    (session, region)
}

/// Systems per page.
fn shape(session: &EditorSession) -> Vec<usize> {
    session
        .resolved()
        .pages
        .iter()
        .map(|page| page.systems.len())
        .collect()
}

#[test]
fn a_line_break_set_before_a_measure_starts_a_system_and_clears() {
    let (mut session, region) = four_measures();
    assert_eq!(shape(&session), vec![1]);
    session
        .set_break(LayoutBreak::System, region, at(2, 1), true)
        .expect("a break before measure 3");
    assert!(session.has_break(LayoutBreak::System, region, &at(2, 1)));
    assert_eq!(shape(&session), vec![2]);
    session
        .set_break(LayoutBreak::System, region, at(2, 1), false)
        .expect("and cleared");
    assert!(!session.has_break(LayoutBreak::System, region, &at(2, 1)));
    assert_eq!(shape(&session), vec![1]);
    session.undo().expect("the clear undoes");
    assert_eq!(shape(&session), vec![2]);
}

#[test]
fn a_page_break_starts_a_page() {
    let (mut session, region) = four_measures();
    session
        .set_break(LayoutBreak::Page, region, at(3, 1), true)
        .expect("a page break before measure 4");
    assert_eq!(shape(&session), vec![1, 1]);
}

#[test]
fn a_break_goes_only_before_a_later_measure() {
    let (mut session, region) = four_measures();
    for place in [at(0, 1), at(1, 2), at(9, 1)] {
        assert_eq!(
            session.set_break(LayoutBreak::System, region, place, true),
            Err(EditorError::NotAMeasureStart)
        );
    }
}

#[test]
fn a_break_toggles_after_the_selected_note_s_measure() {
    let (mut session, region) = four_measures();
    // Select the second measure's note.
    let d = session
        .score()
        .events
        .iter()
        .find(|e| e.position() == &epiphany_core::EventPosition::Musical(at(1, 1)))
        .map(|e| e.id())
        .expect("the second note");
    let layout_object = session
        .hit_test()
        .regions
        .iter()
        .find(|r| match r.source {
            TypedObjectId::Event(e) => e == d,
            TypedObjectId::Pitch(p) => session.score().events.get(d).is_some_and(|e| {
                let mut pitches = Vec::new();
                e.collect_identified_pitches(&mut pitches);
                pitches.iter().any(|ip| ip.id == p)
            }),
            _ => false,
        })
        .map(|r| r.layout_object)
        .expect("the note is drawn");
    session.select(layout_object).expect("selected");
    session
        .toggle_break_after_selection(LayoutBreak::System)
        .expect("a break after measure 2");
    assert!(session.has_break(LayoutBreak::System, region, &at(2, 1)));
    assert_eq!(shape(&session), vec![2]);
    session.select(layout_object);
    session
        .toggle_break_after_selection(LayoutBreak::System)
        .expect("and off again");
    assert!(!session.has_break(LayoutBreak::System, region, &at(2, 1)));
    assert_eq!(shape(&session), vec![1]);
}

/// The region's system breaks as the graph holds them.
fn system_breaks(session: &EditorSession, region: RegionId) -> Vec<epiphany_core::TimeAnchor> {
    session
        .score()
        .canvas
        .regions
        .iter()
        .find(|r| r.id == region)
        .and_then(|r| r.content.staff_based())
        .map(|content| content.user_system_breaks.clone())
        .unwrap_or_default()
}

/// A break is written at the start of the measure it comes before, in the
/// region's first staff instance (reduction version 5 keys a break by its
/// anchor, so it moves with its measure).
#[test]
fn a_break_is_anchored_to_its_measure_s_start() {
    let (mut session, region) = four_measures();
    session
        .set_break(LayoutBreak::System, region, at(2, 1), true)
        .expect("a break before measure 3");
    let third = session
        .score()
        .staff_instances()
        .find(|(r, _)| *r == region)
        .map(|(_, si)| si.measures[2].id)
        .expect("a third measure");
    assert_eq!(
        system_breaks(&session, region),
        [epiphany_core::TimeAnchor::Measure {
            id: third,
            position: epiphany_core::MeasurePosition::Start,
            offset: epiphany_core::AnchorOffset::Zero,
        }]
    );
}

/// A break a document written before reduction version 5 holds, at a musical
/// offset from the region's start, is read as standing there and is cleared
/// in its own form, which is the slot it holds.
#[test]
fn an_older_break_at_an_offset_is_read_and_cleared_in_its_own_form() {
    let (mut session, region) = four_measures();
    let offset = epiphany_core::TimeAnchor::Region {
        id: region,
        edge: epiphany_core::RegionEdge::Start,
        offset: epiphany_core::AnchorOffset::Musical(MusicalDuration(
            RationalTime::new(2, 1).unwrap(),
        )),
    };
    session
        .apply(epiphany_ops::OperationKind::SetUserSystemBreak(
            epiphany_ops::SetUserSystemBreakOp {
                region,
                anchor: offset.clone(),
                present: true,
            },
        ))
        .expect("an offset break applies");
    assert!(session.has_break(LayoutBreak::System, region, &at(2, 1)));
    assert_eq!(shape(&session), vec![2]);
    session
        .set_break(LayoutBreak::System, region, at(2, 1), false)
        .expect("cleared");
    assert!(!session.has_break(LayoutBreak::System, region, &at(2, 1)));
    assert_eq!(system_breaks(&session, region), []);
    assert_eq!(shape(&session), vec![1]);
}
