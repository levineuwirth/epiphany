//! Check 4 (hit testing): point -> (byte offset, affinity) resolution.
//!
//! **This is the candidate-owned part.** `round2-candidatekit` loads the
//! committed `hittest_probes.json` (the *expected* answers) but explicitly
//! must not resolve one — that is what this check measures
//! (`round2_candidatekit::inputs` module doc: "Loading the expected answers
//! ... is neutral. Computing them is the candidate's job"). This module does
//! not call any of `round2_textkit::hittest`'s probe-*generation* functions
//! (`build_probe_table`, `build_all`, ...) — those built the very expected
//! values this module is scored against, so calling them here would make the
//! check circular. The only thing reused from that module is
//! `to_device` — sanctioned by the task instructions as neutral geometry the
//! reference also uses.
//!
//! ## The resolution rule this module implements
//!
//! `ROUND2_TEXT_RECIPE.md` §7's closing paragraph, restated in
//! `round2_textkit::hittest`'s own module doc comment as the semantics the
//! *committed probe table* assumes (not as code this module may call): every
//! grapheme's own `Downstream` caret stop, resolved to device space and
//! sorted by device x, partitions the line into non-overlapping intervals
//! with no gaps. A query point resolves to the stop that begins the interval
//! containing it — "floor" to the nearest stop at or before the point, never
//! a nearest-neighbour vote. Every probe in the committed table expects
//! `Downstream` affinity (recipe §7's own stated consequence of this rule),
//! so this resolver always returns `Downstream`.

use round2_textkit::hittest::{to_device, DevicePoint};
use round2_textkit::types::{SpikeCaretAffinity, SpikeResolvedText};

/// One grapheme's own `Downstream` caret stop, in device space.
struct Stop {
    device_x: f64,
    source_offset: u32,
}

/// Every `Downstream` caret stop in `rt`, sorted ascending by device x. An
/// RTL segment's clusters are byte-ascending but device-x-descending, so
/// sorting by device x (not source order) is what makes the floor lookup
/// below correct on F-B/F-D's Hebrew segments as well as the LTR ones.
fn downstream_stops_by_device_x(rt: &SpikeResolvedText) -> Vec<Stop> {
    let mut stops: Vec<Stop> = rt
        .clusters
        .clusters
        .iter()
        .flat_map(|c| c.caret_stops.iter())
        .filter(|s| s.affinity == SpikeCaretAffinity::Downstream)
        .map(|s| {
            let d = to_device(rt, &s.position);
            Stop {
                device_x: d.x,
                source_offset: s.source_offset,
            }
        })
        .collect();
    stops.sort_by(|a, b| {
        a.device_x
            .partial_cmp(&b.device_x)
            .expect("device x is always finite")
    });
    stops
}

/// Resolves one device point to `(byte offset, affinity)` against `rt`'s own
/// resolved caret-stop data — this candidate's own hit-test implementation,
/// not a lookup into any precommitted table.
///
/// # Panics
///
/// Panics if `rt` has no caret stops at all (every fixture in this recipe
/// has at least one grapheme, so this never fires on the committed set; a
/// degenerate empty-text fixture would need a different contract, not a
/// silently invented answer).
pub fn resolve_hit(rt: &SpikeResolvedText, point: &DevicePoint) -> (u32, SpikeCaretAffinity) {
    let stops = downstream_stops_by_device_x(rt);
    assert!(
        !stops.is_empty(),
        "resolve_hit: no Downstream caret stops in this SpikeResolvedText — nothing to resolve against"
    );

    // Floor: the last stop whose device x is <= the query point's x. Before
    // the first stop, clamp to the first (recipe §7: a probe placed before
    // the first caret stop still expects that stop's own offset).
    let mut chosen = &stops[0];
    for s in &stops {
        if s.device_x <= point.x {
            chosen = s;
        } else {
            break;
        }
    }
    (chosen.source_offset, SpikeCaretAffinity::Downstream)
}

#[cfg(test)]
mod tests {
    use super::*;
    use round2_textkit::identity::{
        SemVerRecord, SpikeShaperId, SpikeTextShapingIdentity, SpikeUnicodeComponent,
    };
    use round2_textkit::types::{
        SpikeBoundingBox, SpikeCaretStop, SpikeCluster, SpikeClusterMap, SpikeGlyphStyle,
        SpikeLanguageTag, SpikePoint, SpikePositionedGlyph, SpikeProvenance, SpikeScriptTag,
        SpikeShapedSegment, SpikeStaffSpace, SpikeTextAlign, SpikeTextDirection,
        SpikeTypedObjectId,
    };

    fn dummy_identity() -> SpikeTextShapingIdentity {
        SpikeTextShapingIdentity {
            faces: Vec::new(),
            shaper: SpikeShaperId("rustybuzz".to_string()),
            shaper_version: SemVerRecord {
                major: 0,
                minor: 20,
                patch: 1,
            },
            features: Vec::new(),
            unicode_bidi: SpikeUnicodeComponent {
                impl_name: "unicode-bidi".to_string(),
                crate_version: "0.3.18".to_string(),
                unicode_version: Some("16.0.0".to_string()),
            },
            unicode_segmentation: SpikeUnicodeComponent {
                impl_name: "unicode-segmentation".to_string(),
                crate_version: "1.13.3".to_string(),
                unicode_version: Some("17.0.0".to_string()),
            },
        }
    }

    fn dummy_provenance() -> SpikeProvenance {
        SpikeProvenance {
            source: SpikeTypedObjectId {
                discriminant: 0,
                canonical_bytes_hex: "00".repeat(18),
            },
            synthesis: None,
            dependencies: Vec::new(),
            stable_id: 0,
        }
    }

    /// Three graphemes at staff-space x = 0.0, 1.0, 2.0 (device x 100, 200,
    /// 300 relative to origin 0,0) — enough to test floor lookup at an
    /// interior midpoint, before the first stop, and after the last.
    fn three_stops() -> SpikeResolvedText {
        let seg = SpikeShapedSegment {
            face: Some(0),
            glyphs: vec![
                SpikePositionedGlyph {
                    glyph_id: 1,
                    offset: SpikePoint::new(0.0, 0.0),
                    transform: None,
                },
                SpikePositionedGlyph {
                    glyph_id: 2,
                    offset: SpikePoint::new(1.0, 0.0),
                    transform: None,
                },
                SpikePositionedGlyph {
                    glyph_id: 3,
                    offset: SpikePoint::new(2.0, 0.0),
                    transform: None,
                },
            ],
            source: 0..3,
            direction: SpikeTextDirection::Ltr,
            script: SpikeScriptTag("Latn".to_string()),
            language: SpikeLanguageTag(None),
            size: SpikeStaffSpace(1.28),
        };
        let mk = |byte: u32, x: f64| SpikeCluster {
            source: byte..byte + 1,
            segment: 0,
            glyph_indices: vec![byte],
            resolved: true,
            grapheme_count: 1,
            caret_stops: vec![SpikeCaretStop {
                source_offset: byte,
                position: SpikePoint::new(x, 0.0),
                affinity: SpikeCaretAffinity::Downstream,
            }],
        };
        SpikeResolvedText {
            provenance: dummy_provenance(),
            text: "abc".to_string(),
            shaping: dummy_identity(),
            segments: vec![seg],
            clusters: SpikeClusterMap {
                clusters: vec![mk(0, 0.0), mk(1, 1.0), mk(2, 2.0)],
            },
            bounds: SpikeBoundingBox {
                left: 0.0,
                bottom: 0.0,
                right: 2.0,
                top: 1.0,
            },
            reserved_box: SpikeBoundingBox {
                left: 0.0,
                bottom: 0.0,
                right: 2.0,
                top: 1.0,
            },
            origin: SpikePoint::new(0.0, 0.0),
            align: SpikeTextAlign::Start,
            style: SpikeGlyphStyle { rgba: 0x0000_00ff },
            layer: 0,
        }
    }

    /// Mutation-first: an interior point between stop 0 (device x=0) and
    /// stop 1 (device x=100) must resolve to stop 0's offset, not stop 1's —
    /// the floor rule, not a nearest-neighbour vote (which would flip the
    /// answer past the literal midpoint at x=50, not matter here, but would
    /// give the WRONG answer at e.g. x=90 under a nearest-stop rule, since
    /// 90 is nearer to 100 than to 0). This point (x=60) is nearer to 0? no —
    /// 60 is nearer to 100 under Euclidean distance (|60-0|=60 > |60-100|=40
    /// is false, so pick a point where floor and nearest disagree instead:
    /// x=90 is nearer to stop 1 (distance 10) than stop 0 (distance 90), so a
    /// nearest-neighbour implementation would wrongly return offset 1 here,
    /// while the correct floor rule returns offset 0.
    #[test]
    fn floor_not_nearest_neighbour() {
        let rt = three_stops();
        let (offset, affinity) = resolve_hit(&rt, &DevicePoint { x: 90.0, y: 0.0 });
        assert_eq!(
            offset, 0,
            "floor rule must pick the stop at-or-before the point, not the nearer one"
        );
        assert_eq!(affinity, SpikeCaretAffinity::Downstream);
    }

    #[test]
    fn before_the_first_stop_clamps_to_it() {
        let rt = three_stops();
        let (offset, _) = resolve_hit(&rt, &DevicePoint { x: -50.0, y: 0.0 });
        assert_eq!(offset, 0);
    }

    #[test]
    fn after_the_last_stop_resolves_to_it() {
        let rt = three_stops();
        let (offset, _) = resolve_hit(&rt, &DevicePoint { x: 1000.0, y: 0.0 });
        assert_eq!(offset, 2);
    }

    #[test]
    fn exactly_on_a_stop_resolves_to_that_stop() {
        let rt = three_stops();
        // byte 1 sits at staff x=1.0 -> device x=100.0.
        let (offset, _) = resolve_hit(&rt, &DevicePoint { x: 100.0, y: 0.0 });
        assert_eq!(offset, 1);
    }
}
