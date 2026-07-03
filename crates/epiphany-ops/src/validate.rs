//! Validation modes and advisory-precondition checking (Chapter 6
//! §"Validation Modes", label `sec:semops:validation`).
//!
//! The spec distinguishes two precondition-checking modes:
//!
//! * **Authoring mode** — interactive edits. *All* preconditions are enforced:
//!   invariant preconditions (which preserve graph invariants) and advisory
//!   preconditions (user-intent constraints, range checks, style policy).
//! * **Replay mode** — replaying historical operations or applying remote
//!   operations under reduction. Only invariant preconditions are enforced;
//!   advisory preconditions MAY fail silently, since they represent the
//!   authoring replica's local policy at the moment of authoring, not
//!   invariants of the canonical state.
//!
//! ## Where each mode lives
//!
//! [`crate::OperationSet::reduce`] and [`crate::OperationSet::reduce_onto`]
//! **are** replay mode: the reducer enforces exactly the invariant
//! preconditions and nothing else, in every context. Authoring-mode
//! enforcement happens **before an envelope is minted** — the authoring layer
//! (epiphany-editor-core) runs [`advisory_violations`] against its current
//! materialized score and refuses to mint on any violation. Canonical
//! reduction behavior and canonical bytes are therefore **untouched** by the
//! mode machinery: an envelope that exists reduces identically whether its
//! author checked advisories or not (the spec's replay-parity requirement).
//!
//! ## The advisory inventory (core spec §6.10)
//!
//! The spec declares advisory preconditions for two of the implemented K0
//! operations (every other implemented kind's precondition bucket is entirely
//! invariant):
//!
//! * **InsertEvent** — (a) for pitched events, every pitch is within the
//!   instrument's declared range, if any; (b) the event's duration does not
//!   extend past a region boundary in a way that would require splitting.
//! * **CreateCrossCutting (Slur case)** — the slur does not span a region
//!   boundary, unless explicitly permitted by region configuration.
//!
//! `ModifyEvent` carries the full replacement event value, so check (b)
//! applies to it identically (the replacement's span must not straddle the
//! region boundary any more than an inserted one may).
//!
//! The Phase-3 first tranche (`CreateStaff`, `SetTimeSignature`,
//! `SetTempoSegment`, `SetStaffLayout` — operation_catalog §CreateStaff,
//! §"Meter and Tempo Overwrites", §SetStaffLayout) declares **no advisory
//! preconditions**: every precondition those entries name (reference
//! resolution, mint freshness, resulting-map well-formedness, target
//! liveness) is invariant and enforced by the reducer in all modes, so this
//! module gains no new checks for them.
//!
//! ### Implemented here
//!
//! * InsertEvent / ModifyEvent duration-not-crossing-region-boundary
//!   ([`AdvisoryViolation::DurationCrossesRegionBoundary`]), for regions whose
//!   musical end bound is resolvable (see below).
//! * CreateCrossCutting(Slur) not-spanning-a-region-boundary
//!   ([`AdvisoryViolation::SlurSpansRegionBoundary`]): the slur's endpoint
//!   events resolve to different regions.
//!
//! ### Documented gaps (blocked on the truncated data model)
//!
//! * **InsertEvent pitch-within-instrument-range**: `epiphany_core::Instrument`
//!   carries only `{ id, name }` — it has no declared range field. The
//!   data-model completion is staged to the Binary Format companion; until the
//!   field exists there is nothing to check against ("if any" in the spec text
//!   makes the absent-range case a vacuous pass, which is exactly what this
//!   module does by omission).
//! * **Slur spanning "explicitly permitted by region configuration"**:
//!   `epiphany_core::Region` has no such configuration flag. The check treats
//!   spanning as never permitted; when the flag lands, it suppresses the
//!   violation.
//! * **Region musical end bound**: a region's `TimeExtent` is a pair of
//!   `TimeAnchor`s. The bound is resolvable in musical time only when the end
//!   anchor is region-start-anchored with a `Musical` offset (the same
//!   sound-but-incomplete resolution discipline as
//!   `epiphany_core::Region::overlaps_in_time`, which resolves only wall-clock
//!   extents). A wall-clock or symbolic extent yields no musical bound and the
//!   boundary check passes vacuously — the full tempo/measure resolution
//!   machinery is deferred (P11-C5).

use epiphany_core::{
    AnchorOffset, EventDuration, EventId, EventPosition, MusicalPosition, Region, RegionEdge,
    RegionId, Score, SlurId, TimeAnchor,
};

use crate::payload::{CrossCuttingValue, OperationKind};
use crate::reduce::graph_voice_location;

/// The two precondition-checking modes of Chapter 6 §"Validation Modes"
/// (`sec:semops:validation`).
///
/// Invariant preconditions hold in **all** modes — the reducer
/// ([`crate::OperationSet::reduce`] / [`crate::OperationSet::reduce_onto`])
/// enforces them unconditionally, and is thereby exactly
/// [`ValidationMode::Replay`]. [`ValidationMode::Authoring`] additionally
/// requires [`advisory_violations`] to be empty *before an envelope is
/// minted*; it never alters reduction.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ValidationMode {
    /// Interactive edits: invariant **and** advisory preconditions are
    /// enforced. The advisory half is enforced pre-mint by the authoring
    /// layer via [`advisory_violations`].
    Authoring,
    /// Replaying historical operations or applying remote operations under
    /// reduction: only invariant preconditions are enforced; advisory
    /// preconditions MAY fail silently.
    Replay,
}

impl ValidationMode {
    /// Whether this mode enforces advisory preconditions (only
    /// [`ValidationMode::Authoring`] does).
    #[inline]
    pub fn enforces_advisory(self) -> bool {
        matches!(self, ValidationMode::Authoring)
    }
}

/// A failed advisory precondition (Chapter 6 §6.10, the "Advisory
/// preconditions (authoring mode only)" buckets).
///
/// **Deliberately non-canonical**: this type has no canonical encoding and no
/// discriminant table because it never enters effects, conflicts, or any other
/// canonical state — it exists only on the authoring side, *before* an
/// envelope is minted. An operation refused for an advisory violation leaves
/// no trace in the operation set; one that slipped past (a remote author's
/// different policy) reduces normally in replay mode.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum AdvisoryViolation {
    /// An `InsertEvent`/`ModifyEvent` event's span starts inside its region's
    /// musical extent but ends past the region's end bound — applying it would
    /// require splitting the event across the boundary (core spec §6.10
    /// InsertEvent, advisory bucket).
    DurationCrossesRegionBoundary {
        /// The offending event.
        event: EventId,
        /// The region whose end bound the event's span crosses.
        region: RegionId,
    },
    /// A `CreateCrossCutting` slur's endpoint events lie in different regions
    /// (core spec §6.10 CreateCrossCutting, Slur advisory bucket). Region
    /// configuration cannot yet permit spanning (see the module docs' gap
    /// list), so a cross-region slur always reports.
    SlurSpansRegionBoundary {
        /// The offending slur.
        slur: SlurId,
        /// The region containing the start event.
        start_region: RegionId,
        /// The (different) region containing the end event.
        end_region: RegionId,
    },
}

/// Checks `kind` against every implemented advisory precondition (the
/// authoring-mode-only bucket of Chapter 6 §6.10), evaluated against `score`
/// — the authoring replica's current materialized graph. Returns every
/// violation found (empty = the operation may be minted in authoring mode).
///
/// Replay mode never calls this: the reducer applies invariant preconditions
/// only, so an envelope violating an advisory check still reduces cleanly
/// (spec: advisory preconditions MAY fail silently in replay mode). The check
/// is deliberately *conservative*: anything it cannot resolve against the
/// graph (a missing voice, a non-musical placement, an unresolvable region
/// bound) passes vacuously and is left to the invariant preconditions under
/// reduction.
pub fn advisory_violations(kind: &OperationKind, score: &Score) -> Vec<AdvisoryViolation> {
    let mut violations = Vec::new();
    match kind {
        OperationKind::InsertEvent(op) => {
            check_event_span(&op.event, score, &mut violations);
        }
        OperationKind::ModifyEvent(op) => {
            check_event_span(&op.event, score, &mut violations);
        }
        OperationKind::CreateCrossCutting(op) => {
            if let CrossCuttingValue::Slur(slur) = &op.structure {
                let start = event_region(score, slur.start_event);
                let end = event_region(score, slur.end_event);
                if let (Some(start_region), Some(end_region)) = (start, end) {
                    if start_region != end_region {
                        violations.push(AdvisoryViolation::SlurSpansRegionBoundary {
                            slur: slur.id,
                            start_region,
                            end_region,
                        });
                    }
                }
            }
        }
        // Every other implemented kind's spec precondition bucket is entirely
        // invariant (see the module docs); nothing to check here.
        _ => {}
    }
    violations
}

/// Reports a violation when `event`'s musical span straddles its region's
/// musical end bound (starts strictly before it, ends strictly past it — the
/// "would require splitting" shape). A non-musical placement, an unlocatable
/// voice, or an unresolvable bound passes vacuously.
fn check_event_span(
    event: &epiphany_core::Event,
    score: &Score,
    violations: &mut Vec<AdvisoryViolation>,
) {
    let (EventPosition::Musical(position), EventDuration::Musical(duration)) =
        (event.position(), event.duration())
    else {
        return;
    };
    let Some((region_index, _, _)) = graph_voice_location(score, event.voice()) else {
        return;
    };
    let region = &score.canvas.regions[region_index];
    let Some(bound) = region_musical_end_bound(region) else {
        return;
    };
    let end = position.clone() + duration.clone();
    if position < &bound && end > bound {
        violations.push(AdvisoryViolation::DurationCrossesRegionBoundary {
            event: event.id(),
            region: region.id,
        });
    }
}

/// The region's end bound as a region-local musical position, when its
/// `TimeExtent`'s end anchor expresses one: anchored to this region's own
/// start edge with a `Musical` offset. Any other shape (wall-clock, symbolic,
/// another region's edge) is not resolvable without the deferred tempo/measure
/// machinery and yields `None` (the advisory check then passes vacuously).
fn region_musical_end_bound(region: &Region) -> Option<MusicalPosition> {
    match &region.time_extent.end {
        TimeAnchor::Region {
            id,
            edge: RegionEdge::Start,
            offset: AnchorOffset::Musical(length),
        } if *id == region.id => Some(MusicalPosition(length.0.clone())),
        _ => None,
    }
}

/// The region containing `event`, resolved through the event's voice. `None`
/// when the event or its voice is not in the graph (the invariant
/// preconditions own that case).
fn event_region(score: &Score, event: EventId) -> Option<RegionId> {
    let ev = score.events.get(event)?;
    let (region_index, _, _) = graph_voice_location(score, ev.voice())?;
    Some(score.canvas.regions[region_index].id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payload::{CreateCrossCuttingOp, InsertEventOp, ModifyEventOp};
    use crate::valuegen;
    use epiphany_core::generators::valid_score;
    use epiphany_core::{
        EventId, MusicalDuration, MusicalPosition, PitchId, RationalTime, ReplicaId, SlurId,
        VoiceId,
    };

    /// A fixture score whose (single) region declares a musical end bound of
    /// `bound` whole units, plus the ids needed to aim operations at it.
    fn bounded_score(bound: i32) -> (Score, RegionId, epiphany_core::StaffInstanceId, VoiceId) {
        let mut score = valid_score(7);
        let region = &mut score.canvas.regions[0];
        let region_id = region.id;
        region.time_extent.end = TimeAnchor::Region {
            id: region_id,
            edge: RegionEdge::Start,
            offset: AnchorOffset::Musical(MusicalDuration(RationalTime::from_int(bound))),
        };
        let instance = region.staff_instances()[0].id;
        let voice = region.staff_instances()[0].voices[0].id;
        (score, region_id, instance, voice)
    }

    fn insert_kind(
        instance: epiphany_core::StaffInstanceId,
        voice: VoiceId,
        position: i32,
        duration: i32,
    ) -> OperationKind {
        OperationKind::InsertEvent(InsertEventOp {
            staff_instance: instance,
            event: valuegen::insert_event_value(
                EventId::new(ReplicaId(50), 999),
                voice,
                MusicalPosition(RationalTime::from_int(position)),
                MusicalDuration(RationalTime::from_int(duration)),
                &[PitchId::new(ReplicaId(50), 998)],
            ),
        })
    }

    #[test]
    fn insert_event_crossing_the_region_end_bound_is_an_advisory_violation() {
        let (score, region, instance, voice) = bounded_score(12);
        // Starts inside the extent (10 < 12), ends past it (14 > 12).
        let kind = insert_kind(instance, voice, 10, 4);
        let violations = advisory_violations(&kind, &score);
        assert_eq!(
            violations,
            vec![AdvisoryViolation::DurationCrossesRegionBoundary {
                event: EventId::new(ReplicaId(50), 999),
                region,
            }]
        );
    }

    #[test]
    fn insert_event_within_the_region_end_bound_passes() {
        let (score, _, instance, voice) = bounded_score(12);
        // Ends exactly at the bound: nothing to split, no violation.
        let kind = insert_kind(instance, voice, 10, 2);
        assert!(advisory_violations(&kind, &score).is_empty());
    }

    #[test]
    fn insert_event_against_an_unresolvable_extent_passes_vacuously() {
        // The unmodified fixture's extent is wall-clock — no musical bound is
        // resolvable, so the boundary check cannot fire (module docs, gap 3).
        let score = valid_score(7);
        let instance = score.canvas.regions[0].staff_instances()[0].id;
        let voice = score.canvas.regions[0].staff_instances()[0].voices[0].id;
        let kind = insert_kind(instance, voice, 10, 1_000);
        assert!(advisory_violations(&kind, &score).is_empty());
    }

    #[test]
    fn modify_event_crossing_the_region_end_bound_is_an_advisory_violation() {
        let (score, region, _, voice) = bounded_score(12);
        // A replacement value for an existing event, moved to straddle the
        // bound. (The advisory check reads the replacement's span; liveness of
        // the target is the reducer's invariant precondition.)
        let target = score.canvas.regions[0].staff_instances()[0].voices[0].events[0];
        let kind = OperationKind::ModifyEvent(ModifyEventOp {
            event: valuegen::insert_event_value(
                target,
                voice,
                MusicalPosition(RationalTime::from_int(11)),
                MusicalDuration(RationalTime::from_int(3)),
                &[PitchId::new(ReplicaId(50), 998)],
            ),
        });
        let violations = advisory_violations(&kind, &score);
        assert_eq!(
            violations,
            vec![AdvisoryViolation::DurationCrossesRegionBoundary {
                event: target,
                region,
            }]
        );
        // The same replacement kept inside the bound passes.
        let kind = OperationKind::ModifyEvent(ModifyEventOp {
            event: valuegen::insert_event_value(
                target,
                voice,
                MusicalPosition(RationalTime::from_int(11)),
                MusicalDuration(RationalTime::from_int(1)),
                &[PitchId::new(ReplicaId(50), 998)],
            ),
        });
        assert!(advisory_violations(&kind, &score).is_empty());
    }

    #[test]
    fn slur_spanning_two_regions_is_an_advisory_violation() {
        // Two single-region fixture scores merged: distinct regions, each with
        // its own events.
        let mut score = valid_score(7);
        let other = valid_score(8);
        let start_region = score.canvas.regions[0].id;
        let end_region = other.canvas.regions[0].id;
        let start_event = score.canvas.regions[0].staff_instances()[0].voices[0].events[0];
        let end_event = other.canvas.regions[0].staff_instances()[0].voices[0].events[0];
        score.canvas.regions.push(other.canvas.regions[0].clone());
        for event in other.events.iter_canonical() {
            score
                .events
                .insert(event.clone())
                .expect("distinct seeds mint distinct event ids");
        }

        let slur_id = SlurId::new(ReplicaId(50), 1);
        let cross = OperationKind::CreateCrossCutting(CreateCrossCuttingOp {
            structure: CrossCuttingValue::Slur(valuegen::slur(slur_id, start_event, end_event)),
        });
        assert_eq!(
            advisory_violations(&cross, &score),
            vec![AdvisoryViolation::SlurSpansRegionBoundary {
                slur: slur_id,
                start_region,
                end_region,
            }]
        );

        // A slur within one region passes.
        let second = score.canvas.regions[0].staff_instances()[0].voices[0].events[1];
        let within = OperationKind::CreateCrossCutting(CreateCrossCuttingOp {
            structure: CrossCuttingValue::Slur(valuegen::slur(slur_id, start_event, second)),
        });
        assert!(advisory_violations(&within, &score).is_empty());
    }

    #[test]
    fn replay_reduction_ignores_advisory_violations_and_is_unchanged_by_the_mode_machinery() {
        // Spec §"Validation Modes": advisory preconditions MAY fail silently
        // in replay mode. An envelope carrying an advisory-violating insert
        // reduces cleanly (Applied) through the ordinary reduction path, and
        // the reduction's canonical bytes are a pure function of the operation
        // set — consulting `advisory_violations` beforehand (as an authoring
        // layer would) changes nothing.
        use crate::causal::CausalContext;
        use crate::stamp::{HybridLogicalClock, OperationStamp};
        use crate::support::AuthorId;
        use crate::{OperationEnvelope, OperationPayload, OperationSet};
        use epiphany_core::{OperationId, WallClockTime};

        let (score, _, instance, voice) = bounded_score(12);
        let kind = insert_kind(instance, voice, 10, 4);
        assert!(
            !advisory_violations(&kind, &score).is_empty(),
            "the scenario must actually violate an advisory precondition"
        );

        let id = OperationId::new(ReplicaId(50), 0);
        let env = OperationEnvelope {
            id,
            author: AuthorId(0),
            stamp: OperationStamp::new(HybridLogicalClock::new(WallClockTime(1), 0), id),
            causal_context: CausalContext::new(),
            transaction: None,
            payload: OperationPayload::Primitive(kind),
        };
        let mut set = OperationSet::new();
        set.accept(env);
        let first = set.reduce_onto(&score);
        assert!(first.state.is_clean(), "replay applies the insert cleanly");
        assert!(matches!(
            first.state.effects.as_slice(),
            [(applied, crate::OperationEffect::Applied)] if *applied == id
        ));
        // Byte-identity across repeated reductions of the same set — the mode
        // machinery has no channel into reduction.
        let second = set.reduce_onto(&score);
        assert_eq!(
            first.state.canonical_bytes(),
            second.state.canonical_bytes()
        );
    }

    #[test]
    fn validation_mode_advisory_enforcement_is_authoring_only() {
        assert!(ValidationMode::Authoring.enforces_advisory());
        assert!(!ValidationMode::Replay.enforces_advisory());
    }
}
