#![forbid(unsafe_code)]
//! # epiphany-engrave
//!
//! Agent I's **engraving constraint solver** (spec **Chapter 9**, "Constraint-
//! Solver Interface"): it turns a [`ConstrainedLayoutIR`] into a
//! [`ResolvedLayoutIR`] with real geometry. It is the production-side replacement
//! for `epiphany-layout-ir`'s interface-only [`StubSolver`] — the QUICKSTART puts
//! the *interface* (`layout-ir`) and the *algorithm* (`engrave`) in separate
//! crates so the core/product boundary stays sharp (`spec/PHASE2_QUICKSTART.md`,
//! crate topology).
//!
//! ## Phase status — honest scaffold, not yet `Minimal`
//!
//! Per the QUICKSTART's recommended development pattern, Agent I develops the
//! renderer ([`epiphany-render-svg`]) against the **stub solver** first, then
//! grows this crate into the real two-pass spring solver. This is the first
//! increment: [`Engraver`] runs a genuine deterministic **horizontal spacing
//! pass** (see [`spacing`]) — the first axis of the planned two-pass spring
//! layout — placing each glyph-bearing slot left-to-right by a collision-aware
//! advance (its preferred width floored by the real glyph bearings) instead of
//! returning the input columns verbatim.
//!
//! It does **not yet** run the vertical spring pass, the soft-constraint
//! stretch/compress solve, or evaluate the IR's declared hard constraints. By the
//! same honesty rule `layout-ir`'s [`StubSolver`] follows, a solver that does not
//! evaluate the declared constraints and computes no quality metrics MUST report
//! [`SolverTier::Stub`], never [`SolverTier::Minimal`] (Chapter 9 §"Conformance
//! Tiers"). So [`Engraver::tier`] reports `Stub` today; it is promoted to
//! `Minimal` in the same change that lands real constraint satisfaction. The
//! quality-metric vector is the conservative all-worst placeholder
//! ([`QualityMetricVector::unmeasured`]) until the Quality Metric Catalog lands
//! (Phase 3).
//!
//! ## Architecture decision (see `DECISIONS.md`)
//!
//! The solver is a **two-pass spring layout** (horizontal then vertical), with
//! the constraint graph derived from the existing [`ConstrainedLayoutIR`]
//! (QUICKSTART decision 2). A global optimization solver is rejected: the spec's
//! deterministic-output requirement makes it expensive to validate.
//!
//! [`epiphany-render-svg`]: ../epiphany_render_svg/index.html

mod spacing;

use epiphany_layout_ir::{
    all_available, BravuraCatalog, ConstrainedLayoutIR, ConstraintSolver, GlyphCatalog,
    InvalidationSet, Margins, Point, QualityMetricVector, Rect, ResolvedGlyph, ResolvedLayoutIR,
    ResolvedPage, ResolvedSystem, Size2D, SolveReport, SolveStatus, SolverBudgetUsed, SolverConfig,
    SolverState, SolverTier, SolverVersion, SolverWarning, SolverWarningKind, Stroke,
};

/// The Epiphany engraving solver (Chapter 9). See the crate docs for the phase
/// status: this is the horizontal-spacing scaffold of the planned two-pass
/// spring layout, reporting [`SolverTier::Stub`] until it earns `Minimal`.
#[derive(Copy, Clone, Debug, Default)]
pub struct Engraver;

/// The implementation version of this solver (Chapter 9: within a fixed version,
/// identical input produces identical output). Distinct from the stub's `0`.
pub const ENGRAVER_VERSION: SolverVersion = SolverVersion(1);

impl Engraver {
    /// Resolves geometry: a deterministic horizontal spacing pass over the spring
    /// slots, copying each glyph to its slot's `x` with its baseline `y`
    /// preserved. Well-formedness is gated exactly as the stub's is — an unknown
    /// glyph, a forged catalog identity, malformed structure, or an explicit
    /// constraint this scaffold cannot yet evaluate yields
    /// [`SolveStatus::InternalError`] (a diagnostic-only layout), never a panic.
    fn resolve(&self, input: &ConstrainedLayoutIR) -> SolveReport {
        let structural_valid = input.validate().is_ok();

        // Short-circuit before catalog construction so an unknown glyph yields a
        // diagnostic, not a panic in the metrics hash (mirrors the stub).
        let names: Vec<&str> = input.glyphs.iter().map(|g| g.glyph.as_str()).collect();
        let metrics_available = all_available(names.iter().copied());
        let catalog_valid = metrics_available && input.catalog == BravuraCatalog.identity(&names);

        // This scaffold does not yet evaluate explicit hard constraints, so — like
        // the stub — it must not claim them satisfied. An input carrying explicit
        // constraints is reported as not-yet-solvable rather than falsely Solved.
        let well_formed = structural_valid && catalog_valid && input.constraints.is_empty();

        // The horizontal spacing pass re-places each glyph by its spring slot.
        // The strokes that track those glyphs (stems, staff lines, barlines) must
        // ride the *same* horizontal map, or a re-spaced notehead would leave its
        // stem behind. Both gate on structural validity: a malformed input must
        // not leak geometry into the diagnostic layout (which reaches
        // canonical_bytes / the renderer).
        let (glyphs, strokes): (Vec<ResolvedGlyph>, Vec<Stroke>) = if structural_valid {
            let remap = HorizontalRemap::build(input);
            (remap.glyphs(input), remap.strokes(input))
        } else {
            (Vec::new(), Vec::new())
        };
        let resolved_glyphs = glyphs.len();

        let mut warnings = Vec::new();
        if structural_valid && catalog_valid && !input.constraints.is_empty() {
            warnings.push(SolverWarning {
                kind: SolverWarningKind::UnusualLayoutDecision(
                    "explicit hard-constraint evaluation is not implemented in the \
                     horizontal-spacing scaffold; it lands with the Minimal tier"
                        .to_owned(),
                ),
                affected_objects: Vec::new(),
                message: "engrave scaffold cannot yet evaluate declared constraints".to_owned(),
            });
        }

        let pages = input
            .regions
            .first()
            .map(|first| ResolvedPage {
                provenance: first.provenance.clone(),
                number: 1,
                size: Size2D::default(),
                margins: Margins::default(),
                systems: input
                    .regions
                    .iter()
                    .map(|region| ResolvedSystem {
                        provenance: region.provenance.clone(),
                        bounding_box: Rect::default(),
                        staves: Vec::new(),
                        measures: Vec::new(),
                    })
                    .collect(),
                free_objects: Vec::new(),
            })
            .into_iter()
            .collect();

        SolveReport {
            status: if well_formed {
                SolveStatus::Solved
            } else {
                SolveStatus::InternalError
            },
            satisfied_hard_constraints: well_formed,
            layout: ResolvedLayoutIR {
                source: input.source,
                pages,
                glyphs,
                strokes,
                engraving_decisions: input.engraving_decisions.clone(),
                catalog: input.catalog.clone(),
            },
            unsatisfied_constraints: Vec::new(),
            warnings,
            // No normalized quality metrics computed yet (Stub tier, Phase 3 work).
            metric_vector: QualityMetricVector::unmeasured(),
            budget_used: SolverBudgetUsed {
                // The horizontal pass touches each slot once; report that honestly.
                iterations: input.horizontal_slots.len() as u64,
                nodes: resolved_glyphs as u64,
                constraint_evaluations: 0,
                wall_time_ms: 0,
            },
            state: SolverState {
                solver_version: Some(self.version()),
                resolved_glyphs,
            },
        }
    }
}

/// A monotonic piecewise-linear map from a constrained x to its spaced x. Each
/// column's *source* x (the baseline its member glyphs share) maps to the
/// *target* x the spacing pass assigns its slot; intermediate and outlying
/// coordinates interpolate/extrapolate linearly. Applied to glyph baselines
/// **and** stroke endpoints alike, so a stroke at (or near) a glyph's column
/// moves with it instead of detaching — the fix for strokes being left at their
/// constrained coordinates while glyphs re-space.
struct HorizontalRemap {
    /// `(source_x, target_x)` control points, sorted by source, sources distinct.
    points: Vec<(f32, f32)>,
}

impl HorizontalRemap {
    fn build(input: &ConstrainedLayoutIR) -> Self {
        // The control points are computed collision-aware (per-slot bearings) by
        // the spacing pass; sources are globally monotonic because regions tile
        // left-to-right.
        HorizontalRemap {
            points: spacing::control_points(input),
        }
    }

    /// Maps a constrained x to its spaced x.
    fn map(&self, x: f32) -> f32 {
        let p = &self.points;
        match p.len() {
            0 => x,
            // One column: a pure translation keeps relative offsets.
            1 => x + (p[0].1 - p[0].0),
            n => {
                if x <= p[0].0 {
                    interp(p[0], p[1], x)
                } else if x >= p[n - 1].0 {
                    interp(p[n - 2], p[n - 1], x)
                } else {
                    p.windows(2)
                        .find(|w| x >= w[0].0 && x <= w[1].0)
                        .map(|w| interp(w[0], w[1], x))
                        .unwrap_or(x)
                }
            }
        }
    }

    /// Re-places each glyph at its mapped x, baseline `y` preserved; provenance,
    /// glyph identity, bounds, style, and layer carried through.
    fn glyphs(&self, input: &ConstrainedLayoutIR) -> Vec<ResolvedGlyph> {
        input
            .glyphs
            .iter()
            .map(|g| ResolvedGlyph {
                provenance: g.provenance.clone(),
                glyph: g.glyph.clone(),
                position: Point::new(self.map(g.baseline.x.0), g.baseline.y.0),
                transform: None,
                bounding_box: g.bounding_box,
                style: g.style,
                layer: g.layer,
            })
            .collect()
    }

    /// Re-maps both endpoints of each stroke, so it tracks the glyphs it spans.
    fn strokes(&self, input: &ConstrainedLayoutIR) -> Vec<Stroke> {
        input
            .strokes
            .iter()
            .map(|s| Stroke {
                provenance: s.provenance.clone(),
                from: Point::new(self.map(s.from.x.0), s.from.y.0),
                to: Point::new(self.map(s.to.x.0), s.to.y.0),
                thickness: s.thickness,
                layer: s.layer,
                style: s.style,
            })
            .collect()
    }
}

/// Linear interpolation/extrapolation through two control points.
fn interp((s0, t0): (f32, f32), (s1, t1): (f32, f32), x: f32) -> f32 {
    if (s1 - s0).abs() < f32::EPSILON {
        t0
    } else {
        t0 + (x - s0) * (t1 - t0) / (s1 - s0)
    }
}

impl ConstraintSolver for Engraver {
    fn tier(&self) -> SolverTier {
        // Honest: a solver that does not evaluate the declared hard constraints
        // and computes no quality metrics is below Minimal (Chapter 9). Promoted
        // to `Minimal` in the change that lands real constraint satisfaction.
        SolverTier::Stub
    }

    fn version(&self) -> SolverVersion {
        ENGRAVER_VERSION
    }

    fn solve(&self, input: &ConstrainedLayoutIR, _config: &SolverConfig) -> SolveReport {
        self.resolve(input)
    }

    fn solve_incremental(
        &self,
        input: &ConstrainedLayoutIR,
        _prior: &SolverState,
        _invalidations: &InvalidationSet,
        _config: &SolverConfig,
    ) -> SolveReport {
        // The scaffold recomputes spacing from scratch, which is trivially
        // observationally equivalent to a scoped incremental solve (Chapter 9
        // §"Observational Equivalence"). Real incremental scoping is Minimal-tier
        // work.
        self.resolve(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use epiphany_core::generators::valid_score_rich;
    use epiphany_layout_ir::{to_constrained, to_logical, StubSolver};

    fn fixture() -> ConstrainedLayoutIR {
        to_constrained(&to_logical(&valid_score_rich(11)))
    }

    #[test]
    fn reports_the_honest_stub_tier_until_it_earns_minimal() {
        // The crate exists to *become* the Minimal solver, but until it evaluates
        // the declared constraints it reports Stub — never a tier it has not
        // earned. This guards the honesty invariant.
        assert_eq!(Engraver.tier(), SolverTier::Stub);
        assert!(Engraver.tier() < SolverTier::Minimal);
        assert_eq!(Engraver.version(), ENGRAVER_VERSION);
        assert_ne!(Engraver.version(), StubSolver.version());
    }

    #[test]
    fn solves_the_stub_pipeline_and_preserves_provenance() {
        let input = fixture();
        let report = Engraver.solve(&input, &SolverConfig::default());
        assert_eq!(report.status, SolveStatus::Solved);
        assert!(report.satisfied_hard_constraints);
        assert_eq!(report.layout.glyphs.len(), input.glyphs.len());
        // Every input glyph's provenance survives, one-for-one.
        for (resolved, original) in report.layout.glyphs.iter().zip(&input.glyphs) {
            assert_eq!(resolved.provenance, original.provenance);
            assert_eq!(resolved.glyph, original.glyph);
        }
        // The metric vector is the honest all-worst placeholder (no metrics yet).
        assert_eq!(report.metric_vector, QualityMetricVector::unmeasured());
    }

    #[test]
    fn a_structurally_invalid_input_emits_no_strokes() {
        // Strokes are gated on the same structural validity as glyphs: an input
        // whose validation fails (here, an out-of-range stroke thickness) yields a
        // diagnostic layout with no glyphs *and* no strokes — the malformed stroke
        // must not leak into canonical_bytes or the renderer.
        let mut input = fixture();
        let provenance = input.glyphs[0].provenance.clone();
        input.strokes.push(epiphany_layout_ir::Stroke {
            provenance,
            from: epiphany_layout_ir::Point::new(0.0, 0.0),
            to: epiphany_layout_ir::Point::new(1.0, 0.0),
            thickness: epiphany_layout_ir::StaffSpace(f32::MAX),
            layer: 0,
            style: epiphany_layout_ir::GlyphStyle::default(),
        });
        assert!(
            input.validate().is_err(),
            "the out-of-range stroke is invalid"
        );
        let report = Engraver.solve(&input, &SolverConfig::default());
        assert_eq!(report.status, SolveStatus::InternalError);
        assert!(report.layout.glyphs.is_empty());
        assert!(
            report.layout.strokes.is_empty(),
            "a structurally invalid input emits no strokes (gated like glyphs)"
        );
    }

    #[test]
    fn horizontal_spacing_differs_from_the_verbatim_stub() {
        // The whole point of the scaffold: it re-spaces horizontally rather than
        // echoing the input columns. With multiple glyphs the positions differ
        // from the stub's verbatim baselines.
        let input = fixture();
        assert!(input.glyphs.len() >= 2);
        let engraved = Engraver.solve(&input, &SolverConfig::default()).layout;
        let stub = StubSolver.solve(&input, &SolverConfig::default()).layout;
        assert_ne!(
            engraved
                .glyphs
                .iter()
                .map(|g| g.position.x.0)
                .collect::<Vec<_>>(),
            stub.glyphs
                .iter()
                .map(|g| g.position.x.0)
                .collect::<Vec<_>>(),
            "the engrave pass must re-space, not echo the stub's columns"
        );
        // ...but it preserves the same glyph set and order.
        assert_eq!(engraved.glyphs.len(), stub.glyphs.len());
    }

    #[test]
    fn strokes_ride_the_same_coordinate_map_as_glyphs() {
        // The spacing pass re-places glyphs; the strokes that track them must move
        // by the same horizontal map, not stay at their constrained coordinates.
        let input = fixture();
        let engraved = Engraver.solve(&input, &SolverConfig::default()).layout;

        // Constrained x -> engraved x, per glyph.
        let glyph_map: Vec<(f32, f32)> = input
            .glyphs
            .iter()
            .zip(&engraved.glyphs)
            .map(|(c, r)| (c.baseline.x.0, r.position.x.0))
            .collect();

        // Every stroke endpoint coincident with a glyph's column lands at that
        // glyph's engraved x — they ride one map, so they stay attached.
        let mut checked = 0;
        for (c, r) in input.strokes.iter().zip(&engraved.strokes) {
            for (gx, ex) in &glyph_map {
                if (c.from.x.0 - gx).abs() < 1e-6 {
                    assert!(
                        (r.from.x.0 - ex).abs() < 1e-3,
                        "a stroke at a glyph's column detached from it after spacing"
                    );
                    checked += 1;
                }
            }
        }
        assert!(
            checked > 0,
            "expected strokes coincident with glyph columns"
        );

        // …and the strokes actually moved (the pass re-spaces, it does not echo).
        let moved = input.strokes.iter().zip(&engraved.strokes).any(|(c, r)| {
            (c.from.x.0 - r.from.x.0).abs() > 1e-4 || (c.to.x.0 - r.to.x.0).abs() > 1e-4
        });
        assert!(
            moved,
            "strokes must be re-spaced with the glyphs, not left behind"
        );
    }

    #[test]
    fn solve_is_deterministic_and_quantizable() {
        let input = fixture();
        let a = Engraver.solve(&input, &SolverConfig::default()).layout;
        let b = Engraver.solve(&input, &SolverConfig::default()).layout;
        // Byte-identical canonical output across solves (Chapter 9 determinism).
        assert_eq!(a.canonical_bytes(), b.canonical_bytes());
    }

    #[test]
    fn engraver_reserves_an_accidental_against_the_previous_note() {
        // A note's accidental overhangs *left* of its notehead, into the previous
        // note's column. The spacing pass must reserve that overhang (against the
        // previous slot's advance), or the accidental overlaps the prior notehead.
        use epiphany_core::{
            AccidentalId, CmnNominal, EventId, MusicalPosition, PitchId, PitchSpelling,
            RationalTime, RegionId, StaffId, TypedObjectId,
        };
        use epiphany_layout_ir::{
            LayoutContent, LayoutObject, LayoutRegion, LocalCoordinateSystem, LogicalLayoutIR,
            MetricTimeAxis, NoteContent, NotePitch, Provenance, ScoreVersion, TimeAxisModel,
            TimePoint, VerticalExtent,
        };

        let region = RegionId::from_raw(1);
        let staff = StaffId::from_raw(10);
        let plain = PitchId::from_raw(100);
        let sharped = PitchId::from_raw(101);
        let manifested = |src, content| {
            LayoutObject::from_projection_with_content(
                Provenance::manifested(src, region, vec![]),
                Some(staff),
                content,
            )
        };
        let at = |n, d| TimePoint::Musical(MusicalPosition(RationalTime::new(n, d).unwrap()));
        let note = |pid: PitchId, time: TimePoint, accidental: bool| {
            let mut spelling = PitchSpelling::cmn(CmnNominal::C, 5);
            if accidental {
                spelling.accidentals.push(AccidentalId::new("sharp"));
            }
            LayoutContent::Note(NoteContent {
                position: time,
                components: vec![],
                pitches: vec![NotePitch {
                    pitch: pid,
                    spelling: Some(spelling),
                }],
            })
        };
        let logical = LogicalLayoutIR {
            source: ScoreVersion::default(),
            regions: vec![LayoutRegion {
                provenance: Provenance::projected(TypedObjectId::Region(region), vec![]),
                coordinate_system: LocalCoordinateSystem::default(),
                time_axis: TimeAxisModel::Metric(MetricTimeAxis::default()),
                vertical_extent: VerticalExtent {
                    staves: vec![staff],
                },
                objects: vec![
                    // A plain note, then a note with a sharp a quarter later.
                    manifested(
                        TypedObjectId::Event(EventId::from_raw(1)),
                        note(plain, at(0, 1), false),
                    ),
                    manifested(TypedObjectId::Pitch(plain), LayoutContent::Structural),
                    manifested(
                        TypedObjectId::Event(EventId::from_raw(2)),
                        note(sharped, at(1, 4), true),
                    ),
                    manifested(TypedObjectId::Pitch(sharped), LayoutContent::Structural),
                ],
            }],
            engraving_decisions: vec![],
            overrides: vec![],
            cross_region: vec![],
        };

        let constrained = to_constrained(&logical);
        let engraved = Engraver
            .solve(&constrained, &SolverConfig::default())
            .layout;
        let mut noteheads: Vec<_> = engraved
            .glyphs
            .iter()
            .filter(|g| g.glyph.as_str() == "noteheadBlack")
            .collect();
        noteheads.sort_by(|a, b| a.position.x.0.partial_cmp(&b.position.x.0).unwrap());
        assert_eq!(noteheads.len(), 2, "two noteheads");
        let first_right = noteheads[0].position.x.0 + noteheads[0].bounding_box.right.0;
        let sharp = engraved
            .glyphs
            .iter()
            .find(|g| g.glyph.as_str() == "accidentalSharp")
            .expect("a sharp is drawn");
        let sharp_left = sharp.position.x.0 + sharp.bounding_box.left.0;
        assert!(
            sharp_left >= first_right,
            "the accidental ({sharp_left}) overlaps the previous notehead's right edge ({first_right})"
        );
    }

    #[test]
    fn engraver_preserves_key_signature_lead_spacing() {
        // The lead area (clef + key signature) is fixed-width content. The spacing
        // pass must reserve it via the lead slot's preferred width, or it compresses
        // the key signature back onto the clef. This drives the *real* engraver, not
        // just the verbatim stub.
        use epiphany_core::{
            CmnNominal, EventId, KeySignature, MusicalPosition, PitchId, PitchSpelling, RegionId,
            StaffId, StaffInstanceId, TypedObjectId,
        };
        use epiphany_layout_ir::{
            LayoutContent, LayoutObject, LayoutRegion, LocalCoordinateSystem, LogicalLayoutIR,
            MetricTimeAxis, NoteContent, NotePitch, PlacedKeySignature, Provenance, ScoreVersion,
            StaffContent, TimeAxisModel, TimePoint, VerticalExtent,
        };

        let region = RegionId::from_raw(1);
        let staff = StaffId::from_raw(10);
        let pitch = PitchId::from_raw(100);
        let manifested = |src, content| {
            LayoutObject::from_projection_with_content(
                Provenance::manifested(src, region, vec![]),
                Some(staff),
                content,
            )
        };
        let logical = LogicalLayoutIR {
            source: ScoreVersion::default(),
            regions: vec![LayoutRegion {
                provenance: Provenance::projected(TypedObjectId::Region(region), vec![]),
                coordinate_system: LocalCoordinateSystem::default(),
                time_axis: TimeAxisModel::Metric(MetricTimeAxis::default()),
                vertical_extent: VerticalExtent {
                    staves: vec![staff],
                },
                objects: vec![
                    // A 3-sharp (A major) key signature, then a note.
                    manifested(
                        TypedObjectId::StaffInstance(StaffInstanceId::from_raw(1)),
                        LayoutContent::Staff(StaffContent {
                            clefs: vec![],
                            keys: vec![PlacedKeySignature {
                                time: TimePoint::Musical(MusicalPosition::origin()),
                                key: KeySignature::new(3).expect("three sharps"),
                            }],
                        }),
                    ),
                    manifested(
                        TypedObjectId::Event(EventId::from_raw(1)),
                        LayoutContent::Note(NoteContent {
                            position: TimePoint::Musical(MusicalPosition::origin()),
                            components: vec![],
                            pitches: vec![NotePitch {
                                pitch,
                                spelling: Some(PitchSpelling::cmn(CmnNominal::C, 5)),
                            }],
                        }),
                    ),
                    manifested(TypedObjectId::Pitch(pitch), LayoutContent::Structural),
                ],
            }],
            engraving_decisions: vec![],
            overrides: vec![],
            cross_region: vec![],
        };

        let constrained = to_constrained(&logical);
        let engraved = Engraver
            .solve(&constrained, &SolverConfig::default())
            .layout;
        let x_of = |name: &str| {
            engraved
                .glyphs
                .iter()
                .find(|g| g.glyph.as_str() == name)
                .map(|g| g.position.x.0)
        };
        let clef_x = x_of("gClef").expect("clef engraved");
        let note_x = x_of("noteheadBlack").expect("notehead engraved");
        let sharps: Vec<f32> = engraved
            .glyphs
            .iter()
            .filter(|g| g.glyph.as_str() == "accidentalSharp")
            .map(|g| g.position.x.0)
            .collect();
        assert_eq!(sharps.len(), 3, "a three-sharp signature");

        // Not compressed into the clef: the lead clearly exceeds one note slot.
        assert!(
            note_x - clef_x > 3.0,
            "key signature compressed into the clef (lead width {})",
            note_x - clef_x
        );
        // The accidentals sit in the lead (clef..note), spread to distinct x.
        assert!(
            sharps.iter().all(|&x| x > clef_x && x < note_x),
            "accidentals must lie between the clef and the first note"
        );
        let mut sorted = sharps;
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert!(
            sorted[0] < sorted[1] && sorted[1] < sorted[2],
            "accidentals are spread out, not stacked at one x"
        );
    }

    #[test]
    fn incremental_is_observationally_equivalent_to_full() {
        let input = fixture();
        let full = Engraver.solve(&input, &SolverConfig::default());
        let inc = Engraver.solve_incremental(
            &input,
            &full.state,
            &InvalidationSet {
                scope: epiphany_layout_ir::InvalidationScope::WholeScore,
                slots: vec![],
                bands: vec![],
                constraints: vec![],
                glyphs: vec![],
            },
            &SolverConfig::default(),
        );
        assert_eq!(full.layout, inc.layout);
    }
}
