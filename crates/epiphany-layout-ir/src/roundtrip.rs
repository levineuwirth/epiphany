//! The layout round-trip (v0 acceptance criterion 6 — Chapter 7's IR contract):
//!
//! > A score graph → LogicalLayoutIR → ConstrainedLayoutIR → stub-solved
//! > ResolvedLayoutIR → RenderIR interface call → back to graph identity with
//! > all provenance preserved.
//!
//! [`round_trip`] runs the whole pipeline and asserts the contract every IR
//! stage must satisfy: the solver reports a renderable status (the stub, which
//! evaluates no constraints, honestly claims hard-constraint satisfaction only
//! for a constraint-free problem; a conformant tier must claim it outright);
//! the **complete** [`Provenance`] of every object survives every stage
//! unchanged; no two objects share a stable id; the stub solver returns the
//! input geometry verbatim; and the *set* of score-graph sources recovered from
//! the RenderIR is exactly the set laid out — a surjection onto graph identity
//! (one source may back several manifestations, each a distinct layout object).
//! This is the contract the testkit's layout harness drives, now against the
//! real crate.

use std::collections::{BTreeMap, BTreeSet};

use epiphany_core::{Score, TypedObjectId};

use crate::constrained::to_constrained;
use crate::logical::{cross_cutting_objects, identified_pitch_ids, to_logical, LayoutObject};
use crate::provenance::{LayoutObjectId, Provenance};
use crate::render::to_render;
use crate::solver::{ConstraintSolver, SolveStatus, SolverConfig, SolverTier, StubSolver};

/// The set of score-graph objects the pipeline lays out — the [`TypedObjectId`]s
/// the round-trip expects to recover from the RenderIR. Kept in lockstep with
/// [`to_logical`]'s projection so the source-set surjection holds.
pub fn laid_out_object_ids(score: &Score) -> BTreeSet<TypedObjectId> {
    let mut ids = BTreeSet::new();
    for region in &score.canvas.regions {
        ids.insert(TypedObjectId::Region(region.id));
        for staff_id in &region.staff_extent.staves {
            ids.insert(TypedObjectId::Staff(*staff_id));
        }
        for si in region.staff_instances() {
            ids.insert(TypedObjectId::StaffInstance(si.id));
            for voice in &si.voices {
                ids.insert(TypedObjectId::Voice(voice.id));
                for eid in &voice.events {
                    ids.insert(TypedObjectId::Event(*eid));
                    for pid in identified_pitch_ids(score, *eid) {
                        ids.insert(TypedObjectId::Pitch(pid));
                    }
                }
            }
            for measure in &si.measures {
                ids.insert(TypedObjectId::Measure(measure.id));
            }
        }
        for go in region.content.graphic_objects() {
            ids.insert(TypedObjectId::GraphicObject(go.id));
        }
    }
    for (src, _deps) in cross_cutting_objects(score) {
        ids.insert(src);
    }
    ids
}

/// What the round-trip recovered, for inspection by tests.
#[derive(Clone, Debug)]
pub struct RoundTripReport {
    pub status: SolveStatus,
    pub logical_objects: usize,
    /// Glyph primitives (one per laid-out glyph).
    pub glyphs: usize,
    /// Stroke primitives (staff lines, stems, markers, …).
    pub render_strokes: usize,
    /// Total render primitives — glyphs **and** strokes.
    pub render_primitives: usize,
    /// Every score-graph source recovered from the RenderIR (glyphs + strokes).
    pub recovered_sources: BTreeSet<TypedObjectId>,
}

/// Collects `stable_id -> Provenance` for a stage's objects, asserting no two
/// objects share a stable id (which would let set comparisons hide duplication).
fn provenance_map<'a>(
    label: &str,
    provenances: impl Iterator<Item = &'a Provenance>,
) -> BTreeMap<LayoutObjectId, Provenance> {
    let mut map = BTreeMap::new();
    for p in provenances {
        let prev = map.insert(p.stable_id, p.clone());
        assert!(
            prev.is_none(),
            "{label}: duplicate stable id {:?} (provenance duplication)",
            p.stable_id
        );
    }
    map
}

/// Runs the full pipeline (acceptance criterion 6): graph → LogicalLayoutIR →
/// ConstrainedLayoutIR → stub-solved ResolvedLayoutIR → RenderIR, asserting it
/// completes without panic and **without losing provenance back-references**.
/// Specifically:
///
/// * the stub solver returns a renderable status, claiming hard-constraint
///   satisfaction exactly when no constraint is declared (it evaluates none);
/// * the complete [`Provenance`] of every object — `source`, `synthesis`,
///   `dependencies`, and `stable_id` — survives every stage unchanged (compared
///   as `stable_id -> Provenance` maps, so a dropped dependency or synthesis
///   kind fails, not just a changed id);
/// * no two objects ever share a `stable_id`, so manifestation multiplicity is
///   preserved through every stage (a source manifested twice stays two layout
///   objects with two stable ids);
/// * the stub solver returns the input geometry verbatim;
/// * the *set* of score-graph sources recovered from the RenderIR equals the set
///   laid out — a surjection onto graph identity (every laid-out source is
///   recovered and nothing spurious appears; one source may back several layout
///   objects, which the distinct stable ids account for).
///
/// Runs against the [`StubSolver`]; [`round_trip_with`] runs the same contract
/// against any conformant solver (a real solver re-spaces, so the verbatim-geometry
/// clause is checked only for the [`SolverTier::Stub`] tier).
pub fn round_trip(score: &Score) -> RoundTripReport {
    round_trip_with(score, &StubSolver)
}

/// Criterion 6 for an arbitrary conformant `solver`: every provenance-preservation
/// guarantee of [`round_trip`] *except* the verbatim-geometry clause, which is the
/// [`SolverTier::Stub`] tier's specific promise. A real solver re-spaces the glyphs,
/// but the [`Provenance`] back-references — `source`, `synthesis`, `dependencies`,
/// and `stable_id` — must survive that re-spacing unchanged, and the recovered
/// source set must still be exactly the set laid out. This is the strictly stronger
/// statement: a solver may move geometry, never lose a provenance trace.
///
/// A conformant solver may also **synthesize** additional objects of its own — a
/// casting-off pass splits a region-spanning staff line into per-system segments —
/// provided each addition declares a [`SynthesisKind`](crate::SynthesisKind) and
/// derives from a source that is already laid out (so the recovered source set is
/// unchanged); the stub passthrough must add nothing.
pub fn round_trip_with<S: ConstraintSolver>(score: &Score, solver: &S) -> RoundTripReport {
    let logical = to_logical(score);
    let constrained = to_constrained(&logical);

    // Full-provenance maps at each stage (duplication is caught while building).
    let logical_map = provenance_map(
        "logical",
        logical
            .regions
            .iter()
            .flat_map(|r| {
                std::iter::once(&r.provenance).chain(r.objects.iter().map(LayoutObject::provenance))
            })
            .chain(logical.cross_region.iter().map(|object| &object.provenance)),
    );
    // Every primitive — glyph *and* stroke — is provenance-tracked. The
    // constrained stage may carry *more* primitives than the logical stage has
    // objects: each logical object is covered by exactly one primitive carrying
    // its provenance, plus the engraver's synthesized derived primitives
    // (accidentals, staff lines, stems, …), whose `source` is constrained to a
    // laid-out object by the surjection below.
    let constrained_map = provenance_map(
        "constrained",
        constrained
            .glyphs
            .iter()
            .map(|g| &g.provenance)
            .chain(constrained.strokes.iter().map(|s| &s.provenance)),
    );
    for (id, provenance) in &logical_map {
        assert_eq!(
            constrained_map.get(id),
            Some(provenance),
            "logical object {id:?} is not covered (with its exact provenance) in constrained"
        );
    }

    let report = solver.solve(&constrained, &SolverConfig::default());
    // A conformant solver need not report exactly `Solved`: any renderable status
    // (`Solved`, `SolvedWithWarnings`, `PartialBudgetExhausted`) carries a layout
    // whose hard constraints are satisfied (Chapter 9 §"The Solver Report"), which
    // is all the round-trip needs — the provenance contract below is independent of
    // quality. The non-renderable, diagnostic-only statuses (Unsatisfiable,
    // InternalError) have no authoritative layout to round-trip.
    assert!(
        report.status.is_renderable(),
        "the solver must return a renderable layout, got {:?}",
        report.status
    );
    if solver.tier() == SolverTier::Stub {
        // The interface-only stub evaluates no constraints, so it may claim
        // hard-constraint satisfaction only for a constraint-free problem —
        // with any declared, an honest report claims none.
        assert_eq!(
            report.satisfied_hard_constraints,
            constrained.constraints.is_empty(),
            "the stub must claim satisfaction exactly when no constraint is declared"
        );
    } else {
        assert!(
            report.satisfied_hard_constraints,
            "a conformant solver must satisfy all hard constraints"
        );
    }

    // The Stub tier's geometry contract: it returns the input geometry *verbatim*
    // — each resolved glyph's position is exactly its constrained baseline (Chapter
    // 9 / QUICKSTART: "the input geometry verbatim"), strokes pass through in order.
    // A higher tier re-spaces, so this clause is the stub's alone; provenance
    // preservation (asserted below) holds for *every* tier regardless.
    if solver.tier() == SolverTier::Stub {
        assert_eq!(
            report.layout.glyphs.len(),
            constrained.glyphs.len(),
            "the stub solver must not add or drop glyphs"
        );
        for (constrained_glyph, resolved_glyph) in
            constrained.glyphs.iter().zip(&report.layout.glyphs)
        {
            assert_eq!(
                resolved_glyph.position, constrained_glyph.baseline,
                "stub solver must return the input geometry verbatim"
            );
            assert_eq!(resolved_glyph.glyph, constrained_glyph.glyph);
            assert_eq!(resolved_glyph.bounding_box, constrained_glyph.bounding_box);
            assert_eq!(resolved_glyph.style, constrained_glyph.style);
            assert_eq!(resolved_glyph.layer, constrained_glyph.layer);
        }
        assert_eq!(
            report.layout.strokes, constrained.strokes,
            "stub solver must return the input strokes verbatim"
        );
    }

    let resolved_map = provenance_map(
        "resolved",
        report
            .layout
            .glyphs
            .iter()
            .map(|g| &g.provenance)
            .chain(report.layout.strokes.iter().map(|s| &s.provenance)),
    );
    // Every constrained object survives into the resolved layout with its exact
    // provenance. A conformant solver may additionally *synthesize* objects of
    // its own — a casting-off pass splits a region-spanning staff line into
    // per-system segments (Chapter 7 §"Provenance": engraver-synthesized
    // objects declare a `SynthesisKind`) — so the resolved map may be a strict
    // superset; the stub tier, a verbatim passthrough, must add nothing.
    for (id, provenance) in &constrained_map {
        assert_eq!(
            resolved_map.get(id),
            Some(provenance),
            "constrained object {id:?} is not preserved (with its exact provenance) in resolved"
        );
    }
    let constrained_sources: BTreeSet<TypedObjectId> =
        constrained_map.values().map(|p| p.source).collect();
    for (id, provenance) in &resolved_map {
        if constrained_map.contains_key(id) {
            continue;
        }
        assert_ne!(
            solver.tier(),
            SolverTier::Stub,
            "the stub passthrough must not add objects (added {id:?})"
        );
        assert!(
            provenance.synthesis.is_some(),
            "solver-added object {id:?} must declare a synthesis kind"
        );
        assert!(
            constrained_sources.contains(&provenance.source),
            "solver-added object {id:?} must derive from a laid-out source, not invent one"
        );
    }

    let render = to_render(&report.layout);
    for (resolved_glyph, primitive) in report.layout.glyphs.iter().zip(&render.primitives) {
        assert_eq!(primitive.glyph, resolved_glyph.glyph);
        assert_eq!(primitive.position, resolved_glyph.position);
        assert_eq!(primitive.transform, resolved_glyph.transform);
        assert_eq!(primitive.bounding_box, resolved_glyph.bounding_box);
        assert_eq!(primitive.style, resolved_glyph.style);
        assert_eq!(primitive.layer, resolved_glyph.layer);
    }
    assert_eq!(
        render.strokes, report.layout.strokes,
        "render must carry the resolved strokes verbatim"
    );
    let render_map = provenance_map(
        "render",
        render
            .primitives
            .iter()
            .map(|p| &p.provenance)
            .chain(render.strokes.iter().map(|s| &s.provenance)),
    );
    assert_eq!(
        resolved_map, render_map,
        "provenance not preserved resolved -> render"
    );

    // Provenance back to graph identity: the recovered source *set* — over every
    // primitive, glyph and stroke — is exactly the set laid out (a surjection:
    // every source recovered, nothing spurious), while the primitive count equals
    // the distinct-stable-id count, so each layout object (and each synthesized
    // derived primitive) is represented exactly once.
    let expected = laid_out_object_ids(score);
    let recovered: BTreeSet<TypedObjectId> = render
        .primitives
        .iter()
        .map(|p| p.provenance.source)
        .chain(render.strokes.iter().map(|s| s.provenance.source))
        .collect();
    assert_eq!(
        expected, recovered,
        "RenderIR sources do not match the laid-out graph objects"
    );
    assert_eq!(
        render.primitives.len() + render.strokes.len(),
        render_map.len(),
        "render produced two primitives with the same stable id"
    );

    RoundTripReport {
        status: report.status,
        logical_objects: logical
            .regions
            .iter()
            .map(|r| 1 + r.objects.len())
            .sum::<usize>()
            + logical.cross_region.len(),
        glyphs: constrained.glyphs.len(),
        render_strokes: render.strokes.len(),
        render_primitives: render.primitives.len() + render.strokes.len(),
        recovered_sources: recovered,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use epiphany_core::generators::{valid_score, valid_score_rich};

    /// A staff manifested in **two** regions traverses the *full pipeline*
    /// (logical → constrained → stub-solved → render) as two distinct layout
    /// objects: two render primitives with the same `source` but distinct stable
    /// ids. Built directly at the IR level (a graph-valid two-region shared-staff
    /// `Score` is awkward to synthesize from the generators — see the note on
    /// `multi_region_scores_have_no_colliding_layout_ids`), so the integration
    /// path itself is covered, not just the id helper.
    #[test]
    fn one_staff_manifested_in_two_regions_round_trips_as_two_objects() {
        use crate::constrained::to_constrained;
        use crate::logical::{LayoutObject, LayoutRegion, LogicalLayoutIR};
        use crate::render::to_render;
        use crate::solver::{ConstraintSolver, SolverConfig, StubSolver};
        use crate::time_axis::{MetricTimeAxis, TimeAxisModel};
        use epiphany_core::{RegionId, StaffId};

        let staff = StaffId::from_raw(5);
        let region = |id: u128| {
            let rid = RegionId::from_raw(id);
            LayoutRegion {
                provenance: Provenance::projected(TypedObjectId::Region(rid), vec![]),
                coordinate_system: crate::LocalCoordinateSystem::default(),
                time_axis: TimeAxisModel::Metric(MetricTimeAxis::default()),
                vertical_extent: crate::VerticalExtent {
                    staves: vec![staff],
                },
                objects: vec![LayoutObject::from_projection(
                    Provenance::manifested(TypedObjectId::Staff(staff), rid, vec![]),
                    Some(staff),
                )],
            }
        };
        let logical = LogicalLayoutIR {
            source: crate::ScoreVersion::default(),
            regions: vec![region(1), region(2)],
            engraving_decisions: vec![],
            overrides: vec![],
            cross_region: vec![],
        };

        let constrained = to_constrained(&logical);
        let report = StubSolver.solve(&constrained, &SolverConfig::default());
        assert_eq!(report.status, SolveStatus::Solved);
        let render = to_render(&report.layout);

        // A staff engraves as stroke line primitives (its five staff lines); its
        // own provenance anchors the bottom line, the upper four are synthesized
        // from it. Two manifestations → two anchor lines with distinct ids.
        let staff_anchors: Vec<_> = render
            .strokes
            .iter()
            .filter(|s| {
                s.provenance.source == TypedObjectId::Staff(staff)
                    && s.provenance.synthesis.is_none()
            })
            .collect();
        assert_eq!(
            staff_anchors.len(),
            2,
            "both manifestations reach the render stage"
        );
        let ids: BTreeSet<_> = staff_anchors
            .iter()
            .map(|s| s.provenance.stable_id)
            .collect();
        assert_eq!(
            ids.len(),
            2,
            "the two manifestations have distinct stable ids"
        );
    }

    #[test]
    fn graph_valid_shared_staff_score_exercises_projection_and_full_pipeline() {
        use epiphany_core::{
            check_invariants, RegionContent, StaffInstance, StaffInstanceId, TimeAnchor,
            TimeExtent, WallClockTime,
        };

        let mut score = valid_score_rich(0x51AFF);
        let shared = score.canvas.regions[0].staff_extent.staves[0];
        score.canvas.regions[1].staff_extent.staves.push(shared);
        let shared_instance: StaffInstanceId = score.identity.mint();
        let RegionContent::StaffBased(content) = &mut score.canvas.regions[1].content else {
            panic!("rich fixture's second region is staff-based");
        };
        content
            .staff_instances
            .push(StaffInstance::new(shared_instance, shared));
        score.canvas.regions[1].time_extent = TimeExtent {
            start: TimeAnchor::WallClock {
                time: WallClockTime(2_000_000),
            },
            end: TimeAnchor::WallClock {
                time: WallClockTime(3_000_000),
            },
        };
        let violations = check_invariants(&score);
        assert!(
            violations.is_empty(),
            "shared-staff fixture must remain graph-valid: {violations:#?}"
        );

        let logical = to_logical(&score);
        let manifestations: Vec<_> = logical
            .regions
            .iter()
            .flat_map(|region| region.objects.iter())
            .filter(|object| object.provenance().source == TypedObjectId::Staff(shared))
            .map(|object| object.provenance().stable_id)
            .collect();
        assert_eq!(manifestations.len(), 2);
        assert_ne!(manifestations[0], manifestations[1]);
        // The pipeline declares real constraints for this score, which the stub
        // does not evaluate — renderable, but not exactly `Solved`.
        let report = round_trip(&score);
        assert!(report.status.is_renderable());
    }

    /// Across a multi-region score, `to_logical` never emits two layout objects
    /// with the same stable id — the manifestation id keys on `(source, region)`,
    /// so even a source reachable from two regions gets two distinct ids and
    /// `round_trip` (which itself asserts no duplicate stable id) does not panic.
    /// The id-distinctness of two manifestations of *one* source is unit-tested
    /// directly in [`crate::provenance`]
    /// (`manifestations_in_distinct_regions_are_distinct`); building a graph-valid
    /// shared-staff score from the generators is fragile (it must satisfy the
    /// region-overlap and coordinate-discipline invariants), so the integration
    /// coverage here is the no-collision property over real multi-region scores.
    #[test]
    fn multi_region_scores_have_no_colliding_layout_ids() {
        for seed in 0..64u64 {
            let score = valid_score_rich(seed);
            let logical = to_logical(&score);
            let mut ids = BTreeSet::new();
            for prov in logical.regions.iter().flat_map(|r| {
                std::iter::once(&r.provenance).chain(r.objects.iter().map(LayoutObject::provenance))
            }) {
                assert!(
                    ids.insert(prov.stable_id),
                    "to_logical emitted two objects with the same stable id"
                );
            }
            // The round-trip holds (its own provenance maps re-assert no dups).
            let _ = round_trip(&score);
        }
    }

    #[test]
    fn valid_scores_round_trip() {
        for seed in 0..64u64 {
            let report = round_trip(&valid_score(seed));
            assert_eq!(
                report.render_primitives,
                report.glyphs + report.render_strokes
            );
            // Every laid-out source is recovered, nothing spurious (the
            // surjection). A source may back several primitives now — a staff's
            // five lines all trace to it — so the recovered *set* is no larger
            // than the primitive count, not equal to it.
            assert_eq!(
                report.recovered_sources,
                laid_out_object_ids(&valid_score(seed))
            );
            assert!(report.recovered_sources.len() <= report.render_primitives);
        }
    }

    #[test]
    fn rich_scores_round_trip_with_cross_cutting() {
        for seed in 0..64u64 {
            let report = round_trip(&valid_score_rich(seed));
            assert!(report.glyphs >= 3);
            // The rich generator carries a tuplet, tie, spanner, marker, and
            // chord symbol — their omission could not pass unseen.
            assert!(report
                .recovered_sources
                .iter()
                .any(|s| matches!(s, TypedObjectId::Tuplet(_))));
            assert!(report
                .recovered_sources
                .iter()
                .any(|s| matches!(s, TypedObjectId::Tie(_))));
        }
    }

    #[test]
    fn reordering_regions_preserves_stable_ids() {
        // A relayout where the *sources* are unchanged must not change any
        // object's stable id (Chapter 7 §"Provenance").
        let score = valid_score_rich(9);
        let source_to_stable = |s: &Score| {
            to_logical(s)
                .regions
                .iter()
                .flat_map(|r| {
                    std::iter::once((r.provenance.source, r.provenance.stable_id)).chain(
                        r.objects
                            .iter()
                            .map(|o| (o.provenance().source, o.provenance().stable_id)),
                    )
                })
                .collect::<BTreeMap<_, _>>()
        };
        let before = source_to_stable(&score);
        let mut reordered = score.clone();
        reordered.canvas.regions.reverse();
        let after = source_to_stable(&reordered);
        assert_eq!(before, after, "reordering regions changed stable ids");
    }
}
