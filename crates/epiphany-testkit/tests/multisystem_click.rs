//! Regression: **multi-system click-to-insert** over the real casting-off
//! engraver.
//!
//! Casting-off wraps the ten-measure QUICKSTART fixture into two stacked
//! systems, each baked back to the page's left margin. The editor's click
//! resolution predated casting-off and assumed one flat system, which broke in
//! two ways: the horizontal inverse gathered a region's anchors across *every*
//! system (an x-non-monotonic list, mapping system-2 clicks to system-1 times),
//! and the vertical inverse always found the staff's *first* line segment (the
//! only one that keeps the manifestation stable id), reading system-2 clicks
//! against system 1's origin. These tests pin the system-aware resolution end
//! to end — `EditorSession` over `Engraver::default()` — where the editor-core
//! unit tests use hand-built geometry.

use std::collections::BTreeMap;

use epiphany_core::{
    CmnNominal, EventPosition, IdentifiedPitch, MusicalDuration, MusicalPosition, PitchId,
    RationalTime, Score, TypedObjectId,
};
use epiphany_editor_core::{EditorSession, GridResolution};
use epiphany_engrave::Engraver;
use epiphany_layout_ir::{Point, Rect, ResolvedSystem};
use epiphany_testkit::fixtures::ten_measure_single_staff;

/// Every pitch's metric onset, from the score graph (the fixture is 40 quarter
/// notes at `k/4`, one pitch per event — the ground truth a click must recover).
fn pitch_onsets(score: &Score) -> BTreeMap<PitchId, MusicalPosition> {
    let mut onsets = BTreeMap::new();
    let mut pitches: Vec<&IdentifiedPitch> = Vec::new();
    for (_, _, voice) in score.voices() {
        for eid in &voice.events {
            let Some(event) = score.events.get(*eid) else {
                continue;
            };
            let EventPosition::Musical(at) = event.position() else {
                continue;
            };
            pitches.clear();
            event.collect_identified_pitches(&mut pitches);
            for ip in &pitches {
                onsets.insert(ip.id, at.clone());
            }
        }
    }
    onsets
}

/// Whether `point` lies within `rect`, edges included.
fn rect_contains(rect: &Rect, point: Point) -> bool {
    point.x.0 >= rect.origin.x.0
        && point.x.0 <= rect.origin.x.0 + rect.size.width.0
        && point.y.0 >= rect.origin.y.0
        && point.y.0 <= rect.origin.y.0 + rect.size.height.0
}

/// The noteheads rendered inside `bounds`, as `(x, pitch)` in ascending x — the
/// non-synthesized pitch-sourced glyphs the horizontal inverse anchors on.
fn noteheads_within(session: &EditorSession, bounds: &Rect) -> Vec<(f32, PitchId)> {
    let mut heads: Vec<(f32, PitchId)> = session
        .resolved()
        .glyphs
        .iter()
        .filter(|g| g.provenance.synthesis.is_none() && rect_contains(bounds, g.position))
        .filter_map(|g| match g.provenance.source {
            TypedObjectId::Pitch(pid) => Some((g.position.x.0, pid)),
            _ => None,
        })
        .collect();
    heads.sort_by(|a, b| a.0.total_cmp(&b.0));
    heads
}

/// A system's staff step origin: the bottom line of its (single) staff **in this
/// system**. The staff record's provenance is that line segment's — the exact
/// world y the vertical inverse must measure from.
fn system_origin_y(session: &EditorSession, system: &ResolvedSystem) -> f32 {
    let staff = system
        .staves
        .first()
        .expect("a cast system records its staff");
    session
        .resolved()
        .strokes
        .iter()
        .find(|s| s.provenance.stable_id == staff.provenance.stable_id)
        .map(|s| s.from.y.0)
        .expect("the staff record's bottom line renders")
}

fn open_two_system_session() -> (EditorSession, BTreeMap<PitchId, MusicalPosition>) {
    let score = ten_measure_single_staff(1);
    let onsets = pitch_onsets(&score);
    let session =
        EditorSession::open(score, Box::new(Engraver::default())).expect("the fixture renders");
    (session, onsets)
}

fn quarter() -> GridResolution {
    GridResolution::quarter()
}

fn eighth() -> GridResolution {
    GridResolution {
        step: MusicalDuration(RationalTime::new(1, 8).expect("1/8 is a valid duration")),
    }
}

/// The two systems the A4 default geometry casts the fixture into (the
/// documented `PageGeometry::default` behavior), page 1 top-first.
fn two_systems(session: &EditorSession) -> (Rect, Rect) {
    let page = session
        .resolved()
        .pages
        .first()
        .expect("the engraver emits a page");
    assert_eq!(
        page.systems.len(),
        2,
        "A4 default geometry wraps the ten-measure fixture into two systems"
    );
    (page.systems[0].bounding_box, page.systems[1].bounding_box)
}

#[test]
fn a_system_2_click_resolves_to_its_own_time_and_pitch() {
    let (session, onsets) = open_two_system_session();
    let (sys1, sys2) = two_systems(&session);

    let sys1_heads = noteheads_within(&session, &sys1);
    let sys2_heads = noteheads_within(&session, &sys2);
    assert!(!sys1_heads.is_empty() && !sys2_heads.is_empty());
    // Sanity: casting really split the run — system 2 carries strictly later music.
    let sys1_max = sys1_heads.iter().map(|(_, p)| &onsets[p]).max().unwrap();
    let sys2_min = sys2_heads.iter().map(|(_, p)| &onsets[p]).min().unwrap();
    assert!(sys2_min > sys1_max, "system 2 renders later onsets");

    let system2 = &session.resolved().pages[0].systems[1];
    let origin = system_origin_y(&session, system2);

    // (b)+(c): a known system-2 notehead — click its x, one staff space above the
    // *system-2* bottom line, and the horizontal inverse must answer that note's
    // onset (not the system-1 time the flat x-scale would give, since system 2
    // restarts at the left margin under system 1's x range).
    let (x, pid) = sys2_heads[0];
    let expected = onsets[&pid].clone();
    let click = Point::new(x, origin + 1.0);
    let gp = session
        .position_at(click, &quarter())
        .expect("a metric position under the click");
    assert_eq!(
        gp.position, expected,
        "a system-2 notehead click snaps to that note's onset"
    );

    // The vertical inverse measures from system 2's own bottom line: one staff
    // space above it under the fixture's default treble clef is G4. (Against
    // system 1's origin — the regression — the same point is ~20 staff spaces
    // below the staff.)
    let pitch = session
        .staff_pitch_at(click)
        .expect("a staff under the click");
    assert_eq!(
        (pitch.nominal, pitch.octave),
        (CmnNominal::G, 4),
        "one staff space above the system-2 bottom line is G4 under treble"
    );

    // A click in the inter-system gutter (just above system 2's box) still
    // resolves — the nearest system by vertical distance — and inverts on
    // system 2's x-scale.
    let gutter = Point::new(x, sys2.origin.y.0 + sys2.size.height.0 + 0.25);
    assert!(
        gutter.y.0 < sys1.origin.y.0 - 0.25,
        "the gutter point is outside both boxes, nearer system 2"
    );
    let from_gutter = session
        .position_at(gutter, &quarter())
        .expect("a between-systems click still resolves");
    assert_eq!(from_gutter.position, expected);
}

#[test]
fn a_system_1_click_still_resolves_as_before() {
    let (session, onsets) = open_two_system_session();
    let (sys1, _) = two_systems(&session);
    let system1 = &session.resolved().pages[0].systems[0];
    let origin = system_origin_y(&session, system1);

    // (e): every system-1 notehead resolves exactly as in the flat layout — its
    // own onset, and G4 one staff space above the bottom line.
    for (x, pid) in noteheads_within(&session, &sys1) {
        let click = Point::new(x, origin + 1.0);
        let gp = session
            .position_at(click, &quarter())
            .expect("a metric position under the click");
        assert_eq!(gp.position, onsets[&pid], "the click snaps to the onset");
        let pitch = session
            .staff_pitch_at(click)
            .expect("a staff under the click");
        assert_eq!((pitch.nominal, pitch.octave), (CmnNominal::G, 4));
    }
}

#[test]
fn insert_into_system_2_empty_space_lands_on_the_clicked_slot() {
    let (mut session, onsets) = open_two_system_session();
    let (_, sys2) = two_systems(&session);

    // (d): the fixture fills every quarter, so the empty space inside system 2 is
    // the off-beat between two of its noteheads. Click halfway between two
    // adjacent system-2 anchors on an eighth grid: the inverse interpolates to
    // the half-beat, and the insert must land there — a system-1 inversion would
    // put it four-plus measures early.
    let (click_x, origin, expected) = {
        let heads = noteheads_within(&session, &sys2);
        assert!(heads.len() >= 2, "system 2 renders adjacent noteheads");
        let (ax, a_pid) = heads[0];
        let (bx, _) = heads[1];
        let system2 = &session.resolved().pages[0].systems[1];
        let expected = onsets[&a_pid].clone()
            + MusicalDuration(RationalTime::new(1, 8).expect("1/8 is a valid duration"));
        (
            (ax + bx) / 2.0,
            system_origin_y(&session, system2),
            expected,
        )
    };
    let click = Point::new(click_x, origin + 1.0);
    let placed = session
        .position_at(click, &eighth())
        .expect("a metric position under the click");
    assert_eq!(
        placed.position, expected,
        "the click names the off-beat slot"
    );

    let outcome = session
        .insert_note_at(click, &eighth())
        .expect("the insert applies (make-room splits the covered quarter)");
    assert!(outcome.graph_changed);

    // The new eighth note exists at the clicked musical position.
    let eighth_dur = MusicalDuration(RationalTime::new(1, 8).expect("1/8 is a valid duration"));
    let landed = session.score().voices().any(|(_, _, voice)| {
        voice.events.iter().any(|eid| {
            session.score().events.get(*eid).is_some_and(|event| {
                event.position() == &EventPosition::Musical(expected.clone())
                    && event.duration()
                        == &epiphany_core::EventDuration::Musical(eighth_dur.clone())
            })
        })
    });
    assert!(landed, "the inserted eighth note sits at the clicked slot");
}
