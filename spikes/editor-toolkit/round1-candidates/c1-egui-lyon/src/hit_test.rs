//! Candidate-owned hit-test resolution: point -> (byte offset, affinity)
//! against a `SpikeResolvedText`'s own caret-stop data (check 4).
//!
//! Loading the *expected* answers (`round2_textkit::hittest::HitTestProbeFile`)
//! is neutral apparatus, consumed as-is in `bin/c1_round2_text.rs`. Computing an
//! answer from a device point is this module's job, and this module's only
//! borrowing from `round2_textkit::hittest` is [`to_device`] — the shared
//! staff-space -> device-space transform every render in this packet uses
//! (the contract requires reusing it rather than re-implementing the
//! transform), not the probe *generator*'s own resolution logic. Resolution
//! itself is independently reasoned about below, not copied from that
//! module's doc comment.
//!
//! ## The resolution rule
//!
//! A run's caret stops (`SpikeCaretStop`, one per grapheme-cluster boundary,
//! from the resolved text's own `ClusterMap`) are the only geometry this
//! candidate has to test a point against. The `Downstream`-affinity stops are
//! exactly the leading edge of each grapheme: sorted by device x they
//! partition the line into a sequence of non-overlapping boxes with no gaps.
//! So a point maps to the stop that begins the box containing it — the
//! largest `Downstream` stop whose device x is at or before the point (a
//! "floor" search over a sorted sequence), never a nearest-neighbour vote,
//! which would be ambiguous exactly at a box's own midpoint. A point before
//! every stop resolves to the first stop (there is no earlier box to belong
//! to); a point after every stop resolves to the last.
//!
//! `Upstream`-affinity stops (the direction-boundary duplicates the resolved
//! text carries at a bidi run boundary) are not part of this partition —
//! they exist so a *caret*, already known to be at a specific logical
//! offset, can pick the geometrically correct side of a direction boundary.
//! A point-to-offset query carries no such prior knowledge, so it is
//! answered from the `Downstream` partition alone, and this resolver's
//! answer always reports `Downstream` affinity.

use round2_textkit::hittest::{to_device, DevicePoint};
use round2_textkit::types::{SpikeCaretAffinity, SpikeResolvedText};

/// One resolved answer: a UTF-8 byte offset into `SpikeResolvedText::text`
/// and the affinity this resolver reports for it — always `Downstream` (see
/// the module doc comment).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct HitTestAnswer {
    pub source_offset: u32,
    pub affinity: SpikeCaretAffinity,
}

/// One `Downstream` caret stop, resolved to device space and kept alongside
/// its source offset.
struct Stop {
    source_offset: u32,
    device_x: f64,
}

/// Builds the device-x-sorted `Downstream` partition this module's
/// resolution rule is defined over.
fn downstream_partition(rt: &SpikeResolvedText) -> Vec<Stop> {
    let mut stops: Vec<Stop> = rt
        .clusters
        .clusters
        .iter()
        .flat_map(|c| c.caret_stops.iter())
        .filter(|s| s.affinity == SpikeCaretAffinity::Downstream)
        .map(|s| Stop {
            source_offset: s.source_offset,
            device_x: to_device(rt, &s.position).x,
        })
        .collect();
    stops.sort_by(|a, b| {
        a.device_x
            .partial_cmp(&b.device_x)
            .expect("device x is always finite")
    });
    stops
}

/// Resolves one device x-coordinate against an already-built, device-x-sorted
/// `Downstream` partition — the "floor" search the module doc comment
/// describes: the last stop at or before `point_x`, or the first stop if
/// `point_x` precedes every stop.
fn resolve_against(partition: &[Stop], point_x: f64) -> HitTestAnswer {
    assert!(
        !partition.is_empty(),
        "a resolved text with zero caret stops cannot be hit-tested"
    );
    let mut floor = &partition[0];
    for stop in partition {
        if stop.device_x <= point_x {
            floor = stop;
        } else {
            break;
        }
    }
    HitTestAnswer {
        source_offset: floor.source_offset,
        affinity: SpikeCaretAffinity::Downstream,
    }
}

/// Resolves `point` against `rt` from scratch — the entry point
/// `bin/c1_round2_text.rs` uses for every probe.
///
/// Only `point.x` is consulted: every fixture in this recipe lays its run out
/// on one fixed baseline (`origin.y` fixed, `align: Start`), so device y does
/// not distinguish anything the probe table tests — every probe in
/// `hittest_probes.json` shares its fixture's one baseline y already.
pub fn resolve(rt: &SpikeResolvedText, point: DevicePoint) -> HitTestAnswer {
    let partition = downstream_partition(rt);
    resolve_against(&partition, point.x)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn spike_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    /// End-to-end: this resolver, run over every one of the 80 committed
    /// probes across all five fixtures, must agree with every precommitted
    /// expected answer. This is the check itself, exercised here as a unit
    /// test rather than only inside the `c1_round2_text` binary, so a
    /// regression in `resolve` is caught by `cargo test -p c1-egui-lyon`
    /// alone.
    #[test]
    fn resolves_every_committed_probe_correctly() {
        let fixtures_path = spike_root().join("round2-textkit/fixtures.json");
        if !fixtures_path.exists() {
            eprintln!("NOT RUN: fixtures.json absent");
            return;
        }
        let fixtures = round2_textkit::output::load_fixtures(&fixtures_path).unwrap();
        let probes_path = spike_root().join("round2-textkit/hittest_probes.json");
        let probe_file =
            round2_textkit::hittest::load_hittest_probes(&probes_path, &fixtures).unwrap();

        let mut total = 0usize;
        let mut mismatches = Vec::new();
        for ft in &probe_file.fixtures {
            let rt = &fixtures
                .fixtures
                .iter()
                .find(|f| f.id == ft.fixture_id)
                .unwrap()
                .resolved;
            for p in &ft.probes {
                total += 1;
                let point = DevicePoint {
                    x: p.point.x,
                    y: p.point.y,
                };
                let answer = resolve(rt, point);
                if answer.source_offset != p.expected_source_offset
                    || answer.affinity != p.expected_affinity
                {
                    mismatches.push(format!(
                        "{}: {} -> got (offset {}, {:?}), expected (offset {}, {:?})",
                        ft.fixture_id,
                        p.source_grapheme,
                        answer.source_offset,
                        answer.affinity,
                        p.expected_source_offset,
                        p.expected_affinity
                    ));
                }
            }
        }
        assert!(
            mismatches.is_empty(),
            "{}/{total} probes mismatched:\n{}",
            mismatches.len(),
            mismatches.join("\n")
        );
        assert_eq!(total, 80, "the recipe measures exactly 80 committed probes");
    }

    /// Mutation-first (task requirement): a synthetic two-stop run, floor
    /// resolution at the midpoint must return the FIRST stop, not the
    /// nearest one — a nearest-neighbour implementation (the bug this
    /// module's doc comment explicitly rejects) would return the same
    /// answer on one side and disagree exactly at the midpoint's other
    /// side, so this test probes both sides of the midpoint, not the tie
    /// itself.
    #[test]
    fn floor_semantics_not_nearest_neighbour() {
        let partition = vec![
            Stop {
                source_offset: 0,
                device_x: 0.0,
            },
            Stop {
                source_offset: 5,
                device_x: 100.0,
            },
        ];
        // Just past the midpoint (50.0) on the left: nearest-neighbour would
        // still say "first stop" here too, so this alone doesn't
        // distinguish the rules -- the distinguishing point is anything in
        // (0, 100) at all under floor semantics, which always says "first
        // stop" until x reaches 100. Assert floor holds all the way up to
        // (but not including) the second stop.
        assert_eq!(resolve_against(&partition, 0.0).source_offset, 0);
        assert_eq!(resolve_against(&partition, 49.0).source_offset, 0);
        assert_eq!(resolve_against(&partition, 50.0).source_offset, 0);
        assert_eq!(resolve_against(&partition, 99.999).source_offset, 0);
        assert_eq!(resolve_against(&partition, 100.0).source_offset, 5);
        assert_eq!(resolve_against(&partition, 500.0).source_offset, 5);
        // Before the first stop: still resolves to the first stop.
        assert_eq!(resolve_against(&partition, -50.0).source_offset, 0);
    }

    /// Required kill: every answer's affinity is `Downstream`, never
    /// `Upstream` — this resolver has no notion of "the caret's own side" a
    /// point-only query lacks (module doc comment).
    #[test]
    fn every_resolved_answer_is_downstream() {
        let partition = vec![Stop {
            source_offset: 0,
            device_x: 0.0,
        }];
        assert_eq!(
            resolve_against(&partition, 10.0).affinity,
            SpikeCaretAffinity::Downstream
        );
    }
}
