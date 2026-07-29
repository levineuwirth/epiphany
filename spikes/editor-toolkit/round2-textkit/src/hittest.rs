//! The hit-test probe table (`ROUND2_TEXT_RECIPE.md` §7 closing paragraph):
//! a committed, per-fixture table of `(device point) -> (byte offset,
//! affinity)` probes, derived from `SpikeResolvedText`'s own caret-stop data
//! via recipe §3's transform.
//!
//! **This is candidate-testing apparatus, not part of the candidate-neutral
//! §3E mirror** — `crate::fixtures`/`crate::output` build and validate
//! `SpikeResolvedText` itself; this module only *reads* an already-built,
//! already-validated [`crate::output::FixtureFile`] and derives a table from
//! it. That is why it lives in a sibling file, `hittest_probes.json`, rather
//! than as extra fields on `fixtures.json`'s own records — see
//! `bin/generate_hittest.rs`'s doc comment for the full reasoning.
//!
//! ## The hit-test semantics this table commits to
//!
//! The recipe names the *rule for generating probes* ("midpoint of each
//! adjacent grapheme... at least 4 device px from any stop") but not the
//! *hit-test semantics a probe's expected answer assumes*. This module picks
//! the simplest one that is well-defined everywhere a probe can legally sit
//! (i.e. everywhere the 4 px floor allows a probe at all): **a device point
//! maps to the caret stop that begins the grapheme whose box contains it** —
//! graphemes' own `Downstream` caret stops, sorted by device x, partition the
//! whole line into non-overlapping boxes with no gaps, so this is a pure
//! interval lookup ("floor" to the nearest stop at or before the point),
//! never a nearest-neighbour vote. A point exactly at the *literal* midpoint
//! between two stops (rather than a stop-to-stop interval interior) would be
//! a genuine 50/50 tie under a nearest-caret rule; the interval-floor
//! semantics used here have no such tie anywhere strictly inside a box, which
//! is exactly what lets every probe carry one unambiguous expected answer.
//!
//! **Consequence, stated plainly: every probe's expected affinity is
//! `Downstream`.** `Upstream` caret stops (the direction-boundary extras
//! `crate::shape::inject_direction_boundary_stops` adds — F-D bytes 8 and 14)
//! are deliberately excluded from probe generation. Measured on F-D's own
//! data: every `Upstream` stop's device position coincides *exactly* with
//! some other grapheme's own `Downstream` box boundary elsewhere in the
//! fixture (byte 8's and byte 14's `Upstream` positions both equal byte 12's
//! `Downstream` position, 4.609375 staff-space x — segment 1's own
//! `start_pen`, referenced from both sides of the RTL run). A probe placed
//! near an `Upstream` stop's position is therefore a probe placed near an
//! *already-covered* `Downstream` box boundary — exactly the kind of position
//! the 4 px separation rule exists to keep every probe away from. This table
//! cannot exercise affinity disambiguation at a direction boundary without
//! violating its own separation invariant; that property is committed and
//! checked elsewhere (`invariants::assert_direction_boundary_stops_differ`,
//! already asserted on every loaded fixture by `output::FixtureFile::validate`).
//! Recorded here as a finding, not silently worked around.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::output::FixtureFile;
use crate::types::{SpikeCaretAffinity, SpikePoint, SpikeResolvedText};

/// Device pixels per staff space (recipe §3): `scale = 100`.
pub const DEVICE_SCALE: f64 = crate::DEVICE_SCALE;

/// Recipe §7: "Points are placed at least 4 device px from any stop
/// position." Not a tunable — a probe violating this is dropped, never
/// accepted with a smaller margin (see [`build_probe_table`]'s doc comment).
pub const MIN_STOP_SEPARATION_DEVICE_PX: f64 = 4.0;

/// How far past the first/last caret stop the two edge probes sit. Not
/// pinned by the recipe (which only requires clearing the 4 px floor); fixed
/// here at 20 px — five times the floor — so the edge probes are nowhere
/// near a rounding tie on any fixture in this set (every measured interior
/// gap is at least 31.9 device px; see this module's generation output).
pub const EDGE_MARGIN_DEVICE_PX: f64 = 20.0;

/// The five fixture ids, in order — restated (not read back from
/// `crate::fixtures::FIXTURES`), the same discipline
/// `output::EXPECTED_FIXTURES` uses and for the same reason: a validator
/// built from a different copy of this crate must still catch drift.
pub const EXPECTED_FIXTURE_IDS: [&str; 5] = ["F-A", "F-B", "F-C", "F-D", "F-E"];

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DevicePoint {
    pub x: f64,
    pub y: f64,
}

fn distance(a: &DevicePoint, b: &DevicePoint) -> f64 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt()
}

/// Converts a position **relative to `rt.origin`** (the convention every
/// `SpikePositionedGlyph::offset` and `SpikeCaretStop::position` already
/// uses) to device pixels, via recipe §3's transform: `device = (staff.x *
/// scale, target_height/2 - staff.y * scale)`.
///
/// This adds `rt.origin` first rather than using the recipe's nominal
/// `(160, 540)` as an additive device-space constant — the two are *not*
/// quite the same thing. `rt.origin` is the honestly-**quantized** origin
/// (`crate::quantize`'s doc comment: `1638/1024 = 1.599609375`, not the raw
/// literal `1.6`), so the run's baseline actually sits at device
/// `(159.9609375, 540)`, not `(160, 540)`. The discrepancy is under
/// 0.04 device px — far below anything that matters for an 8+ px probe
/// margin — but this module computes it exactly rather than silently
/// re-introducing the same rounded-literal shortcut `crate::quantize`'s
/// module doc comment names as a bug shape to avoid.
pub fn to_device(rt: &SpikeResolvedText, relative: &SpikePoint) -> DevicePoint {
    let staff_x = rt.origin.x + relative.x;
    let staff_y = rt.origin.y + relative.y;
    DevicePoint {
        x: staff_x * DEVICE_SCALE,
        y: crate::TARGET_HEIGHT / 2.0 - staff_y * DEVICE_SCALE,
    }
}

/// Every caret stop in `rt`, in device space, **both affinities** — this is
/// what a probe must clear the [`MIN_STOP_SEPARATION_DEVICE_PX`] floor
/// against (recipe §7: "at least 4 device px from any stop position" — any,
/// not just the two stops bounding the interior interval a probe was
/// generated from).
fn all_stop_device_points(rt: &SpikeResolvedText) -> Vec<DevicePoint> {
    rt.clusters
        .clusters
        .iter()
        .flat_map(|c| c.caret_stops.iter())
        .map(|s| to_device(rt, &s.position))
        .collect()
}

/// One grapheme's own leading-edge (`Downstream`) caret stop, resolved to
/// device space, kept in **device-x-sorted (visual) order** — see this
/// module's doc comment for why this order, not source-byte order, is the
/// one probe generation needs (an RTL segment's clusters are byte-ascending
/// but device-x-descending).
struct GraphemeEntry {
    source_offset: u32,
    device: DevicePoint,
}

fn downstream_sequence(rt: &SpikeResolvedText) -> Vec<GraphemeEntry> {
    let mut v: Vec<GraphemeEntry> = rt
        .clusters
        .clusters
        .iter()
        .flat_map(|c| c.caret_stops.iter())
        .filter(|s| s.affinity == SpikeCaretAffinity::Downstream)
        .map(|s| GraphemeEntry {
            source_offset: s.source_offset,
            device: to_device(rt, &s.position),
        })
        .collect();
    v.sort_by(|a, b| {
        a.device
            .x
            .partial_cmp(&b.device.x)
            .expect("device x is always finite")
    });
    v
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum ProbeKind {
    /// Generated from the midpoint of two device-x-adjacent `Downstream`
    /// stops (recipe §7: "for every caret stop, one probe at the midpoint of
    /// each adjacent grapheme" — see the module doc comment for why this is
    /// realized as one probe per adjacent *pair*, not two identical probes
    /// per interior grapheme).
    Interior,
    /// Before the first caret stop (recipe §7).
    BeforeFirst,
    /// After the last caret stop (recipe §7).
    AfterLast,
}

/// One committed probe: a device point, its expected hit-test answer, and
/// which grapheme/interval it was generated from (traceability — task
/// requirement).
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HitTestProbe {
    pub point: DevicePoint,
    pub expected_source_offset: u32,
    pub expected_affinity: SpikeCaretAffinity,
    /// Human-readable provenance: which fixture, which grapheme/interval,
    /// and which probe kind. Not machine-checked itself (the geometry and
    /// expected offset/affinity are); a report or a `FAIL` names the probe by
    /// this string rather than an opaque index.
    pub source_grapheme: String,
    pub kind: ProbeKind,
}

/// A probe the generator refused to emit because it could not clear the
/// [`MIN_STOP_SEPARATION_DEVICE_PX`] floor — recipe §7: "do not shrink the
/// margin — drop that probe and RECORD that it was dropped, with which
/// fixture and why."
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DroppedProbe {
    pub fixture_id: String,
    pub point: DevicePoint,
    pub nearest_stop_distance_device_px: f64,
    pub reason: String,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureProbeTable {
    pub fixture_id: String,
    pub probes: Vec<HitTestProbe>,
}

/// `hittest_probes.json`'s root document.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HitTestProbeFile {
    pub contract: String,
    pub recipe: String,
    pub min_separation_device_px: f64,
    pub edge_margin_device_px: f64,
    pub fixtures: Vec<FixtureProbeTable>,
    pub dropped: Vec<DroppedProbe>,
}

/// Considers one candidate probe: accepts it into `probes` if it clears the
/// separation floor against **every** caret stop in `all_stops` (both
/// affinities), otherwise records it in `dropped` with the measured distance
/// and never lowers the floor to make it fit (recipe §7, restated in this
/// module's own doc comment).
fn consider(
    fixture_id: &str,
    all_stops: &[DevicePoint],
    point: DevicePoint,
    expected_source_offset: u32,
    expected_affinity: SpikeCaretAffinity,
    kind: ProbeKind,
    label: String,
    probes: &mut Vec<HitTestProbe>,
    dropped: &mut Vec<DroppedProbe>,
) {
    let min_dist = all_stops
        .iter()
        .map(|s| distance(&point, s))
        .fold(f64::INFINITY, f64::min);
    if min_dist < MIN_STOP_SEPARATION_DEVICE_PX {
        dropped.push(DroppedProbe {
            fixture_id: fixture_id.to_string(),
            point,
            nearest_stop_distance_device_px: min_dist,
            reason: format!(
                "{label}: nearest caret stop is only {min_dist:.4} device px away, below the \
                 {MIN_STOP_SEPARATION_DEVICE_PX} px floor recipe §7 requires — dropped rather \
                 than shrinking the margin"
            ),
        });
    } else {
        probes.push(HitTestProbe {
            point,
            expected_source_offset,
            expected_affinity,
            source_grapheme: label,
            kind,
        });
    }
}

/// Builds one fixture's probe table (recipe §7): one probe per pair of
/// device-x-adjacent graphemes (their shared interior interval — see the
/// module doc comment for why this, not two probes per stop, is the correct
/// non-redundant realization of "midpoint of each adjacent grapheme"), plus
/// one probe before the first stop and one after the last, each checked
/// against the [`MIN_STOP_SEPARATION_DEVICE_PX`] floor and dropped (not
/// shrunk) if it fails.
pub fn build_probe_table(
    fixture_id: &str,
    rt: &SpikeResolvedText,
) -> (FixtureProbeTable, Vec<DroppedProbe>) {
    let seq = downstream_sequence(rt);
    let all_stops = all_stop_device_points(rt);
    let mut probes = Vec::new();
    let mut dropped = Vec::new();

    for w in seq.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        let mid = DevicePoint {
            x: (a.device.x + b.device.x) / 2.0,
            y: a.device.y,
        };
        let label = format!(
            "{fixture_id}: interior, grapheme starting at byte {} (interval byte {}..byte {})",
            a.source_offset, a.source_offset, b.source_offset
        );
        consider(
            fixture_id,
            &all_stops,
            mid,
            a.source_offset,
            SpikeCaretAffinity::Downstream,
            ProbeKind::Interior,
            label,
            &mut probes,
            &mut dropped,
        );
    }

    if let Some(first) = seq.first() {
        let point = DevicePoint {
            x: first.device.x - EDGE_MARGIN_DEVICE_PX,
            y: first.device.y,
        };
        let label = format!(
            "{fixture_id}: before the first caret stop (byte {})",
            first.source_offset
        );
        consider(
            fixture_id,
            &all_stops,
            point,
            first.source_offset,
            SpikeCaretAffinity::Downstream,
            ProbeKind::BeforeFirst,
            label,
            &mut probes,
            &mut dropped,
        );
    }
    if let Some(last) = seq.last() {
        let point = DevicePoint {
            x: last.device.x + EDGE_MARGIN_DEVICE_PX,
            y: last.device.y,
        };
        let label = format!(
            "{fixture_id}: after the last caret stop (byte {})",
            last.source_offset
        );
        consider(
            fixture_id,
            &all_stops,
            point,
            last.source_offset,
            SpikeCaretAffinity::Downstream,
            ProbeKind::AfterLast,
            label,
            &mut probes,
            &mut dropped,
        );
    }

    (
        FixtureProbeTable {
            fixture_id: fixture_id.to_string(),
            probes,
        },
        dropped,
    )
}

/// Builds every fixture's probe table from an already-loaded, already-valid
/// [`FixtureFile`] (`crate::output::load_fixtures` validates before this ever
/// sees it).
pub fn build_all(fixtures: &FixtureFile) -> (Vec<FixtureProbeTable>, Vec<DroppedProbe>) {
    let mut tables = Vec::with_capacity(fixtures.fixtures.len());
    let mut all_dropped = Vec::new();
    for f in &fixtures.fixtures {
        let (table, mut dropped) = build_probe_table(&f.id, &f.resolved);
        tables.push(table);
        all_dropped.append(&mut dropped);
    }
    (tables, all_dropped)
}

pub fn build_hittest_probe_file(fixtures: &FixtureFile) -> HitTestProbeFile {
    let (tables, dropped) = build_all(fixtures);
    HitTestProbeFile {
        contract: "spec/CONTRACT_EDITOR_T4_SPIKE.md pins 8, 9, 10, 13, 14".to_string(),
        recipe: "spikes/editor-toolkit/ROUND2_TEXT_RECIPE.md §7".to_string(),
        min_separation_device_px: MIN_STOP_SEPARATION_DEVICE_PX,
        edge_margin_device_px: EDGE_MARGIN_DEVICE_PX,
        fixtures: tables,
        dropped,
    }
}

impl HitTestProbeFile {
    /// Checks the loaded file against literals restated here, then against a
    /// **fresh recomputation** from `fixtures` — the same two-tier discipline
    /// `output::FixtureFile::validate` uses (structural/literal checks, plus
    /// re-deriving the checkable facts rather than trusting the file's own
    /// other fields).
    ///
    /// The per-probe 4 px separation check runs first and independently
    /// (against `fixtures`'s own caret-stop positions, recomputed fresh, never
    /// against a self-declared distance field this file could carry
    /// un-audited) so a probe moved too close to a stop fails with a specific,
    /// on-topic message rather than the generic "recomputation disagrees"
    /// one below it.
    pub fn validate(&self, fixtures: &FixtureFile) -> Result<(), String> {
        if self.min_separation_device_px != MIN_STOP_SEPARATION_DEVICE_PX {
            return Err(format!(
                "min_separation_device_px is {}, recipe §7 fixes it at {MIN_STOP_SEPARATION_DEVICE_PX}",
                self.min_separation_device_px
            ));
        }
        if self.edge_margin_device_px != EDGE_MARGIN_DEVICE_PX {
            return Err(format!(
                "edge_margin_device_px is {}, this crate fixes it at {EDGE_MARGIN_DEVICE_PX}",
                self.edge_margin_device_px
            ));
        }
        if self.fixtures.len() != EXPECTED_FIXTURE_IDS.len() {
            return Err(format!(
                "{} fixture probe tables recorded, recipe §2 names {} fixtures",
                self.fixtures.len(),
                EXPECTED_FIXTURE_IDS.len()
            ));
        }
        for (i, expected_id) in EXPECTED_FIXTURE_IDS.iter().enumerate() {
            if self.fixtures[i].fixture_id != *expected_id {
                return Err(format!(
                    "fixtures[{i}] id is {:?}, recipe §2 names {expected_id:?}",
                    self.fixtures[i].fixture_id
                ));
            }
            if self.fixtures[i].probes.is_empty() {
                return Err(format!(
                    "{expected_id}: no probes recorded — every fixture must have probes (recipe §7)"
                ));
            }
        }

        for ft in &self.fixtures {
            let resolved = &fixtures
                .fixtures
                .iter()
                .find(|f| f.id == ft.fixture_id)
                .ok_or_else(|| {
                    format!(
                        "{}: not present in the supplied fixtures file",
                        ft.fixture_id
                    )
                })?
                .resolved;
            let stops = all_stop_device_points(resolved);
            for p in &ft.probes {
                let d = stops
                    .iter()
                    .map(|s| distance(&p.point, s))
                    .fold(f64::INFINITY, f64::min);
                if d < MIN_STOP_SEPARATION_DEVICE_PX {
                    return Err(format!(
                        "{}: probe {:?} (expects byte {} / {:?}) sits {d:.4} device px from its \
                         nearest caret stop, below the {MIN_STOP_SEPARATION_DEVICE_PX} px floor \
                         recipe §7 requires — a rounding tie could pass or fail it either way",
                        ft.fixture_id, p.point, p.expected_source_offset, p.expected_affinity
                    ));
                }
            }
        }

        let (expected_fixtures, expected_dropped) = build_all(fixtures);
        if self.fixtures != expected_fixtures {
            return Err(
                "hit-test probe table disagrees with a fresh recomputation from the supplied \
                 fixtures — the committed file has drifted from the data it was built from"
                    .to_string(),
            );
        }
        if self.dropped != expected_dropped {
            return Err(format!(
                "dropped-probe list disagrees with a fresh recomputation: {} recorded, {} expected",
                self.dropped.len(),
                expected_dropped.len()
            ));
        }

        Ok(())
    }
}

/// Loads `hittest_probes.json` and validates it against `fixtures` in one
/// call — the same "no public load-without-validating" discipline
/// `output::load_fixtures` establishes, for the same reason.
pub fn load_hittest_probes(
    path: &Path,
    fixtures: &FixtureFile,
) -> Result<HitTestProbeFile, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read hit-test probes at {}: {e}", path.display()))?;
    let file: HitTestProbeFile = serde_json::from_str(&text)
        .map_err(|e| format!("failed to parse hit-test probes at {}: {e}", path.display()))?;
    file.validate(fixtures).map_err(|e| {
        format!(
            "hit-test probes at {} failed validation: {e}",
            path.display()
        )
    })?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{
        SemVerRecord, SpikeShaperId, SpikeTextShapingIdentity, SpikeUnicodeComponent,
    };
    use crate::output;
    use crate::types::{
        SpikeBoundingBox, SpikeCaretStop, SpikeCluster, SpikeClusterMap, SpikeGlyphStyle,
        SpikeLanguageTag, SpikePositionedGlyph, SpikeProvenance, SpikeScriptTag,
        SpikeShapedSegment, SpikeStaffSpace, SpikeTextAlign, SpikeTextDirection,
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
            source: crate::types::SpikeTypedObjectId {
                discriminant: 0,
                canonical_bytes_hex: "00".repeat(18),
            },
            synthesis: None,
            dependencies: Vec::new(),
            stable_id: 0,
        }
    }

    /// Three graphemes at staff-space x = 0.0, 1.0, 1.02 — the last gap is
    /// 0.02 staff space = 2 device px, so its interior midpoint sits 1 device
    /// px from each of its bounding stops, below the 4 px floor. Used to
    /// prove [`build_probe_table`] actually drops a too-close probe rather
    /// than silently shrinking the margin (task requirement).
    fn fixture_with_a_narrow_gap() -> SpikeResolvedText {
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
                    offset: SpikePoint::new(1.02, 0.0),
                    transform: None,
                },
            ],
            source: 0..3,
            direction: SpikeTextDirection::Ltr,
            script: SpikeScriptTag("Latn".to_string()),
            language: SpikeLanguageTag(None),
            size: SpikeStaffSpace(1.28),
        };
        let mk_cluster = |byte: u32, x: f64| SpikeCluster {
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
                clusters: vec![mk_cluster(0, 0.0), mk_cluster(1, 1.0), mk_cluster(2, 1.02)],
            },
            bounds: SpikeBoundingBox {
                left: 0.0,
                bottom: 0.0,
                right: 1.0,
                top: 1.0,
            },
            reserved_box: SpikeBoundingBox {
                left: 0.0,
                bottom: 0.0,
                right: 1.0,
                top: 1.0,
            },
            origin: SpikePoint::new(0.0, 0.0),
            align: SpikeTextAlign::Start,
            style: SpikeGlyphStyle { rgba: 0x0000_00ff },
            layer: 0,
        }
    }

    #[test]
    fn a_probe_within_the_floor_is_dropped_and_recorded_not_shrunk() {
        let rt = fixture_with_a_narrow_gap();
        let (table, dropped) = build_probe_table("T-NARROW", &rt);
        // Two interior intervals: byte0..byte1 (gap 100 device px, fine) and
        // byte1..byte2 (gap 2 device px, must be dropped).
        assert_eq!(
            dropped.len(),
            1,
            "exactly the narrow byte1..byte2 interval must be dropped, got {dropped:?}"
        );
        assert_eq!(dropped[0].fixture_id, "T-NARROW");
        assert!(
            dropped[0].nearest_stop_distance_device_px < MIN_STOP_SEPARATION_DEVICE_PX,
            "recorded distance must actually be below the floor: {:?}",
            dropped[0]
        );
        assert!(
            dropped[0].reason.contains("T-NARROW"),
            "{}",
            dropped[0].reason
        );
        assert!(
            dropped[0].reason.contains("below the 4"),
            "{}",
            dropped[0].reason
        );
        // The surviving probes must never include the dropped point's
        // interval — i.e. no probe expects byte 1 from an Interior kind at
        // this narrow gap. (byte 1 still legitimately appears from the wide
        // byte0..byte1 interval and is not itself excluded.)
        let narrow_interval_survived = table.probes.iter().any(|p| {
            p.kind == ProbeKind::Interior
                && p.expected_source_offset == 1
                && p.source_grapheme.contains("byte 1..byte 2")
        });
        assert!(
            !narrow_interval_survived,
            "the narrow interval's probe must not appear among the accepted probes: {:#?}",
            table.probes
        );
    }

    #[test]
    fn a_wide_gap_produces_an_accepted_probe_at_its_midpoint() {
        let rt = fixture_with_a_narrow_gap();
        let (table, _dropped) = build_probe_table("T-NARROW", &rt);
        let wide = table
            .probes
            .iter()
            .find(|p| p.kind == ProbeKind::Interior && p.expected_source_offset == 0)
            .expect("the wide byte0..byte1 interval must survive");
        // origin (0,0) + relative (0.5, 0) staff -> device x = 50.0
        assert!((wide.point.x - 50.0).abs() < 1e-9, "{:?}", wide.point);
        assert_eq!(wide.expected_affinity, SpikeCaretAffinity::Downstream);
    }

    fn committed_fixtures() -> Option<FixtureFile> {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures.json");
        if !p.exists() {
            return None;
        }
        Some(output::load_fixtures(&p).expect("committed fixtures.json must load and validate"))
    }

    #[test]
    fn a_freshly_built_probe_file_validates() {
        let Some(fixtures) = committed_fixtures() else {
            eprintln!("NOT RUN: fixtures.json absent");
            return;
        };
        let file = build_hittest_probe_file(&fixtures);
        file.validate(&fixtures).unwrap();
    }

    #[test]
    fn every_committed_fixture_has_probes_and_zero_are_dropped() {
        // Measured fact, restated as a check: every interior gap in the five
        // committed fixtures is at least 31.9 device px (see this crate's
        // generation output), comfortably clearing the 4 px floor, so a
        // freshly built table drops nothing. If this ever starts dropping
        // probes, that is itself a finding worth surfacing, not a silently
        // absorbed change.
        let Some(fixtures) = committed_fixtures() else {
            eprintln!("NOT RUN: fixtures.json absent");
            return;
        };
        let file = build_hittest_probe_file(&fixtures);
        for ft in &file.fixtures {
            assert!(!ft.probes.is_empty(), "{} has no probes", ft.fixture_id);
        }
        assert!(
            file.dropped.is_empty(),
            "expected zero drops on the committed fixture set, got {:#?}",
            file.dropped
        );
    }

    /// Mutation-first (task requirement): move one probe to within 4 px of a
    /// stop and confirm `validate` rejects it with the specific
    /// separation-floor message, not the generic recomputation-drift one.
    #[test]
    fn validate_kills_a_probe_moved_within_the_floor() {
        let Some(fixtures) = committed_fixtures() else {
            eprintln!("NOT RUN: fixtures.json absent");
            return;
        };
        let mut file = build_hittest_probe_file(&fixtures);
        let fa = &mut file.fixtures[0];
        assert_eq!(fa.fixture_id, "F-A");
        // F-A byte0's own Downstream stop sits at device x = 160.0 (relative
        // x 0.0 + origin.x 1.599609375, times scale 100 = 159.9609375).
        // Move the first accepted probe to 2 device px away from it —
        // inside the 4 px floor.
        fa.probes[0].point.x = 159.9609375 + 2.0;
        let err = file.validate(&fixtures).unwrap_err();
        assert!(err.contains("below the 4"), "{err}");
        assert!(err.contains("device px"), "{err}");
    }

    /// Mutation-first (task requirement): change one expected byte offset
    /// and confirm `validate` catches it via the recomputation-drift check.
    #[test]
    fn validate_kills_a_wrong_expected_offset() {
        let Some(fixtures) = committed_fixtures() else {
            eprintln!("NOT RUN: fixtures.json absent");
            return;
        };
        let mut file = build_hittest_probe_file(&fixtures);
        let fa = &mut file.fixtures[0];
        assert_eq!(fa.fixture_id, "F-A");
        fa.probes[0].expected_source_offset += 1;
        let err = file.validate(&fixtures).unwrap_err();
        assert!(
            err.contains("disagrees with a fresh recomputation"),
            "{err}"
        );
    }

    #[test]
    fn validate_kills_a_wrong_min_separation_literal() {
        let Some(fixtures) = committed_fixtures() else {
            eprintln!("NOT RUN: fixtures.json absent");
            return;
        };
        let mut file = build_hittest_probe_file(&fixtures);
        file.min_separation_device_px = 1.0;
        let err = file.validate(&fixtures).unwrap_err();
        assert!(err.contains("min_separation_device_px"), "{err}");
    }

    #[test]
    fn validate_kills_a_wrong_edge_margin_literal() {
        let Some(fixtures) = committed_fixtures() else {
            eprintln!("NOT RUN: fixtures.json absent");
            return;
        };
        let mut file = build_hittest_probe_file(&fixtures);
        file.edge_margin_device_px = 5.0;
        let err = file.validate(&fixtures).unwrap_err();
        assert!(err.contains("edge_margin_device_px"), "{err}");
    }

    #[test]
    fn validate_kills_a_missing_fixture_table() {
        let Some(fixtures) = committed_fixtures() else {
            eprintln!("NOT RUN: fixtures.json absent");
            return;
        };
        let mut file = build_hittest_probe_file(&fixtures);
        file.fixtures.pop();
        let err = file.validate(&fixtures).unwrap_err();
        assert!(err.contains("fixture probe tables"), "{err}");
    }

    #[test]
    fn validate_kills_an_empty_probe_list() {
        let Some(fixtures) = committed_fixtures() else {
            eprintln!("NOT RUN: fixtures.json absent");
            return;
        };
        let mut file = build_hittest_probe_file(&fixtures);
        file.fixtures[0].probes.clear();
        let err = file.validate(&fixtures).unwrap_err();
        assert!(err.contains("no probes recorded"), "{err}");
    }

    #[test]
    fn validate_kills_an_extra_spurious_probe() {
        let Some(fixtures) = committed_fixtures() else {
            eprintln!("NOT RUN: fixtures.json absent");
            return;
        };
        let mut file = build_hittest_probe_file(&fixtures);
        let extra = file.fixtures[0].probes[0].clone();
        file.fixtures[0].probes.push(extra);
        let err = file.validate(&fixtures).unwrap_err();
        assert!(
            err.contains("disagrees with a fresh recomputation"),
            "{err}"
        );
    }

    #[test]
    fn json_round_trip_preserves_validity() {
        let Some(fixtures) = committed_fixtures() else {
            eprintln!("NOT RUN: fixtures.json absent");
            return;
        };
        let file = build_hittest_probe_file(&fixtures);
        let json = serde_json::to_string_pretty(&file).unwrap();
        let reloaded: HitTestProbeFile = serde_json::from_str(&json).unwrap();
        reloaded.validate(&fixtures).unwrap();
    }

    #[test]
    fn an_unknown_field_is_refused() {
        let Some(fixtures) = committed_fixtures() else {
            eprintln!("NOT RUN: fixtures.json absent");
            return;
        };
        let file = build_hittest_probe_file(&fixtures);
        let mut v: serde_json::Value = serde_json::to_value(&file).unwrap();
        v.as_object_mut()
            .unwrap()
            .insert("smuggled_field".into(), serde_json::json!(1));
        let err = serde_json::from_value::<HitTestProbeFile>(v)
            .unwrap_err()
            .to_string();
        assert!(err.contains("smuggled_field"), "{err}");
    }

    /// Every probe's expected affinity is `Downstream` (this module's own
    /// documented consequence of interval-floor semantics) — checked here so
    /// the claim in the module doc comment cannot silently stop being true.
    #[test]
    fn every_probe_expects_downstream_affinity() {
        let Some(fixtures) = committed_fixtures() else {
            eprintln!("NOT RUN: fixtures.json absent");
            return;
        };
        let file = build_hittest_probe_file(&fixtures);
        for ft in &file.fixtures {
            for p in &ft.probes {
                assert_eq!(
                    p.expected_affinity,
                    SpikeCaretAffinity::Downstream,
                    "{}: probe {:?} expects a non-Downstream affinity",
                    ft.fixture_id,
                    p
                );
            }
        }
    }
}
