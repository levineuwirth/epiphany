//! The layout round-trip harness (v0 acceptance criterion 6 — Chapter 7's IR
//! contract):
//!
//! > A score graph → LogicalLayoutIR → stub-solved ResolvedLayoutIR → RenderIR
//! > interface call completes without panic and without losing provenance
//! > back-references.
//!
//! **Agent E (`epiphany-layout-ir`) has landed.** This module was formerly a
//! faithful in-tree *stub* of the layout IR; per the QUICKSTART re-point note it
//! now drives the **real** crate, re-exporting its IR types behind the same
//! [`round_trip`] signature the acceptance suite already calls. The
//! provenance-preservation contract the harness asserts is implemented and
//! tested inside `epiphany-layout-ir` itself; the testkit keeps deterministic
//! generators for E's public types (Agent F's "generators for every public type
//! in A through E" mandate) and exercises the real `round_trip` on the testkit's
//! hand-off fixtures.
//!
//! The module name is retained so the harness entry point stays
//! `layout_stub::round_trip`.

// Re-export the real layout IR: the four stages, `TimeAxisModel`, the provenance
// back-references, the glyph-catalog identity, the engraving-decision and
// vertical-band models, the edit-barrier types, and the stub solver.
pub use epiphany_layout_ir::*;

use crate::rng::Rng;

// --- Deterministic generators for Agent E's public types (Agent F mandate) ---

/// A generated layout-object identifier value.
pub fn gen_layout_object_id(rng: &mut Rng) -> LayoutObjectId {
    LayoutObjectId(((rng.next_u64() as u128) << 64) | rng.next_u64() as u128)
}

/// A synthesis kind (every variant, including the registered form).
pub fn gen_synthesis_kind(rng: &mut Rng) -> SynthesisKind {
    match rng.below(7) {
        0 => SynthesisKind::CancellationAccidental,
        1 => SynthesisKind::KeySignatureNatural,
        2 => SynthesisKind::GeneratedRest,
        3 => SynthesisKind::EngravedBreak,
        4 => SynthesisKind::MultimeasureRest,
        5 => SynthesisKind::Cautionary,
        _ => SynthesisKind::Registered(SynthesisRegistryId(
            ((rng.next_u64() as u128) << 64) | rng.next_u64() as u128,
        )),
    }
}

/// A time-axis model (every variant of the tagged enum).
pub fn gen_time_axis_model(rng: &mut Rng) -> TimeAxisModel {
    match rng.below(4) {
        0 => TimeAxisModel::Metric(MetricTimeAxis::default()),
        1 => TimeAxisModel::Proportional(ProportionalTimeAxis {
            duration_ns: rng.range(0, 1 << 40) as i64,
            space_per_second: gen_staff_space(rng),
            placements: vec![],
        }),
        2 => TimeAxisModel::Aleatoric(AleatoricTimeAxis::default()),
        _ => TimeAxisModel::Registered(
            TimeAxisRegistryId(((rng.next_u64() as u128) << 64) | rng.next_u64() as u128),
            SerializedRegisteredAxis(vec![rng.next_u64() as u8]),
        ),
    }
}

/// A provenance record with a random source and a few dependency
/// back-references, **internally consistent**: a synthesized provenance carries
/// `Some(kind)` *and* the matching `synthesized_layout_id`; a projected one
/// carries `None` *and* `stable_layout_id`. (It never sets a synthesis kind while
/// keeping the plain source-only id.)
pub fn gen_provenance(rng: &mut Rng) -> Provenance {
    let source = crate::generators::typed_object_id(rng);
    let mut dependencies = Vec::new();
    for _ in 0..rng.range_usize(0, 3) {
        dependencies.push(crate::generators::typed_object_id(rng));
    }
    if rng.boolean() {
        let kind = gen_synthesis_kind(rng);
        Provenance::synthesized(
            source,
            kind,
            SynthesisInstanceKey(((rng.next_u64() as u128) << 64) | rng.next_u64() as u128),
            dependencies,
        )
    } else {
        Provenance::projected(source, dependencies)
    }
}

/// A bundled SMuFL glyph name.
fn gen_glyph_name(rng: &mut Rng) -> &'static str {
    BRAVURA_METRICS[rng.below(BRAVURA_METRICS.len() as u64) as usize]
        .name
        .as_ref()
}

/// A solver status (every variant).
pub fn gen_solve_status(rng: &mut Rng) -> SolveStatus {
    *rng.choose(&[
        SolveStatus::Solved,
        SolveStatus::SolvedWithWarnings,
        SolveStatus::PartialBudgetExhausted,
        SolveStatus::Unsatisfiable,
        SolveStatus::InternalError,
    ])
}

/// A glyph-catalog identity (the bundled Bravura identity).
pub fn gen_glyph_catalog_identity(_rng: &mut Rng) -> GlyphCatalogIdentity {
    GlyphCatalogIdentity::default()
}

/// A 2-D staff-space point.
pub fn gen_point(rng: &mut Rng) -> Point {
    // Spread points over a few hundred staff spaces at sub-grid resolution.
    let coord = |r: &mut Rng| (r.range(0, 400_000) as f32) / 1000.0;
    Point::new(coord(rng), coord(rng))
}

/// A logical layout object (provenance + an optional owning staff).
pub fn gen_layout_object(rng: &mut Rng) -> LayoutObject {
    LayoutObject::from_projection(
        gen_provenance(rng),
        rng.boolean().then(|| crate::generators::staff_id(rng)),
    )
}

/// A constrained glyph object (provenance + glyph + baseline anchor + band).
pub fn gen_glyph_object(rng: &mut Rng) -> GlyphObject {
    let glyph_name = gen_glyph_name(rng);
    GlyphObject {
        provenance: gen_provenance(rng),
        glyph: GlyphReference::borrowed(glyph_name),
        horizontal_slot: gen_spring_slot_id(rng),
        baseline: gen_point(rng),
        vertical_band: gen_vertical_band_id(rng),
        bounding_box: metrics(glyph_name)
            .expect("generator chooses a bundled glyph")
            .bounding_box(),
        anchor: gen_point(rng),
        layer: rng.range(0, 8) as i32,
        style: GlyphStyle {
            rgba: rng.next_u64() as u32,
        },
    }
}

/// A resolved glyph (provenance + glyph + definitive position).
pub fn gen_resolved_glyph(rng: &mut Rng) -> ResolvedGlyph {
    ResolvedGlyph {
        provenance: gen_provenance(rng),
        glyph: GlyphReference::borrowed(gen_glyph_name(rng)),
        position: gen_point(rng),
        transform: None,
        bounding_box: gen_glyph_bounding_box(rng),
        style: GlyphStyle {
            rgba: rng.next_u64() as u32,
        },
        layer: rng.range(0, 8) as i32,
    }
}

/// A logical layout region (provenance + time axis + a few objects).
pub fn gen_layout_region(rng: &mut Rng) -> LayoutRegion {
    let n = rng.range_usize(0, 4);
    let region = crate::generators::region_id(rng);
    let objects: Vec<LayoutObject> = (0..n).map(|_| gen_layout_object(rng)).collect();
    let staves = objects.iter().filter_map(LayoutObject::staff).collect();
    LayoutRegion {
        provenance: Provenance::projected(epiphany_core::TypedObjectId::Region(region), vec![]),
        coordinate_system: LocalCoordinateSystem::default(),
        time_axis: gen_time_axis_model(rng),
        vertical_extent: VerticalExtent { staves },
        objects,
    }
}

/// A logical layout IR (a few regions and engraving decisions).
pub fn gen_logical_layout_ir(rng: &mut Rng) -> LogicalLayoutIR {
    let n = rng.range_usize(0, 3);
    let d = rng.range_usize(0, 3);
    LogicalLayoutIR {
        source: ScoreVersion::default(),
        regions: (0..n).map(|_| gen_layout_region(rng)).collect(),
        engraving_decisions: (0..d).map(|_| gen_engraving_decision(rng)).collect(),
        overrides: vec![],
        cross_region: vec![],
    }
}

/// A constrained layout IR whose catalog hash covers exactly its glyph metrics
/// (so the real stub solver accepts it as well-formed), with an **internally
/// consistent** vertical band: every glyph names the band, and the band's
/// members are exactly those glyphs.
pub fn gen_constrained_layout_ir(rng: &mut Rng) -> ConstrainedLayoutIR {
    let n = rng.range_usize(0, 8);
    let mut glyphs: Vec<GlyphObject> = (0..n).map(|_| gen_glyph_object(rng)).collect();
    // One band owning exactly these glyphs; point every glyph at it.
    let band = VerticalBand::staff(
        crate::generators::staff_id(rng),
        glyphs.iter().map(|g| g.id()).collect(),
    );
    for g in &mut glyphs {
        g.vertical_band = band.id;
    }
    let horizontal_slots: Vec<SpringSlot> = glyphs
        .iter_mut()
        .enumerate()
        .map(|(index, glyph)| {
            let id = SpringSlotId(glyph.id().0);
            glyph.horizontal_slot = id;
            SpringSlot {
                id,
                time: TimePoint::WallClock(epiphany_core::WallClockTime(index as i64)),
                min_width: StaffSpace(1.0),
                preferred_width: StaffSpace(1.5),
                max_width: None,
                stretch_factor: 1.0,
                compress_factor: 1.0,
                members: vec![glyph.id()],
            }
        })
        .collect();
    let metrics_hash = metrics_hash_for(glyphs.iter().map(|g| g.glyph.as_str()));
    let decisions = rng.range_usize(0, 3);
    ConstrainedLayoutIR {
        source: ScoreVersion::default(),
        regions: vec![],
        horizontal_slots,
        glyphs,
        vertical_bands: vec![band],
        constraints: vec![],
        engraving_decisions: (0..decisions)
            .map(|_| gen_engraving_decision(rng))
            .collect(),
        catalog: GlyphCatalogIdentity {
            metrics_hash,
            ..GlyphCatalogIdentity::default()
        },
    }
}

/// A resolved layout IR produced by the real stub-solver interface.
pub fn gen_resolved_layout_ir(rng: &mut Rng) -> ResolvedLayoutIR {
    StubSolver
        .solve(&gen_constrained_layout_ir(rng), &SolverConfig::default())
        .layout
}

pub fn gen_resolved_staff(rng: &mut Rng) -> ResolvedStaff {
    let staff = crate::generators::staff_id(rng);
    ResolvedStaff {
        provenance: Provenance::projected(epiphany_core::TypedObjectId::Staff(staff), vec![]),
        staff,
        bounding_box: gen_rect(rng),
    }
}

pub fn gen_resolved_measure(rng: &mut Rng) -> ResolvedMeasure {
    let measure = crate::generators::measure_id(rng);
    ResolvedMeasure {
        provenance: Provenance::projected(epiphany_core::TypedObjectId::Measure(measure), vec![]),
        measure,
        bounding_box: gen_rect(rng),
    }
}

pub fn gen_resolved_system(rng: &mut Rng) -> ResolvedSystem {
    ResolvedSystem {
        provenance: gen_provenance(rng),
        bounding_box: gen_rect(rng),
        staves: (0..rng.range_usize(0, 2))
            .map(|_| gen_resolved_staff(rng))
            .collect(),
        measures: (0..rng.range_usize(0, 2))
            .map(|_| gen_resolved_measure(rng))
            .collect(),
    }
}

pub fn gen_resolved_page(rng: &mut Rng) -> ResolvedPage {
    ResolvedPage {
        provenance: gen_provenance(rng),
        number: rng.range(1, 500) as u32,
        size: gen_size2d(rng),
        margins: gen_margins(rng),
        systems: (0..rng.range_usize(0, 2))
            .map(|_| gen_resolved_system(rng))
            .collect(),
        free_objects: (0..rng.range_usize(0, 2))
            .map(|_| gen_glyph_object_id(rng))
            .collect(),
    }
}

/// A render primitive with generated provenance, glyph, and geometry.
pub fn gen_render_primitive(rng: &mut Rng) -> RenderPrimitive {
    RenderPrimitive {
        provenance: gen_provenance(rng),
        glyph: GlyphReference::borrowed(gen_glyph_name(rng)),
        position: gen_point(rng),
        transform: None,
        bounding_box: gen_glyph_bounding_box(rng),
        style: GlyphStyle {
            rgba: rng.next_u64() as u32,
        },
        layer: rng.range(0, 8) as i32,
    }
}

/// A RenderIR generated through the resolved-to-render projection.
pub fn gen_render_ir(rng: &mut Rng) -> RenderIR {
    to_render(&gen_resolved_layout_ir(rng))
}

/// A complete solver report generated through the solver interface.
pub fn gen_solve_report(rng: &mut Rng) -> SolveReport {
    StubSolver.solve(&gen_constrained_layout_ir(rng), &SolverConfig::default())
}

/// A self-consistent round-trip report value.
pub fn gen_round_trip_report(rng: &mut Rng) -> RoundTripReport {
    let render = gen_render_ir(rng);
    let recovered_sources = render
        .primitives
        .iter()
        .map(|primitive| primitive.provenance.source)
        .collect();
    RoundTripReport {
        status: SolveStatus::Solved,
        logical_objects: render.primitives.len(),
        glyphs: render.primitives.len(),
        render_primitives: render.primitives.len(),
        recovered_sources,
    }
}

// --- Generators for the rest of E's public surface (Agent F mandate) ---

/// A staff-space measurement.
pub fn gen_staff_space(rng: &mut Rng) -> StaffSpace {
    StaffSpace((rng.range(0, 400_000) as f32) / 1000.0)
}

/// A 2-D staff-space size.
pub fn gen_size2d(rng: &mut Rng) -> Size2D {
    Size2D {
        width: gen_staff_space(rng),
        height: gen_staff_space(rng),
    }
}

/// A staff-space bounding box.
pub fn gen_bounding_box(rng: &mut Rng) -> BoundingBox {
    let c = |r: &mut Rng| (r.range(0, 8000) as f32) / 1000.0;
    BoundingBox::new(c(rng), c(rng), c(rng), c(rng))
}

pub fn gen_rect(rng: &mut Rng) -> Rect {
    Rect {
        origin: gen_point(rng),
        size: gen_size2d(rng),
    }
}

pub fn gen_transform_2d(rng: &mut Rng) -> Transform2D {
    let mut transform = Transform2D::default();
    transform.matrix[0][2] = gen_staff_space(rng).0;
    transform.matrix[1][2] = gen_staff_space(rng).0;
    transform
}

pub fn gen_margins(rng: &mut Rng) -> Margins {
    Margins {
        top: gen_staff_space(rng),
        right: gen_staff_space(rng),
        bottom: gen_staff_space(rng),
        left: gen_staff_space(rng),
    }
}

pub fn gen_score_version(rng: &mut Rng) -> ScoreVersion {
    let mut bytes = [0; 32];
    for chunk in bytes.chunks_exact_mut(8) {
        chunk.copy_from_slice(&rng.next_u64().to_le_bytes());
    }
    ScoreVersion(bytes)
}

pub fn gen_local_coordinate_system(rng: &mut Rng) -> LocalCoordinateSystem {
    LocalCoordinateSystem {
        transform: gen_transform_2d(rng),
    }
}

pub fn gen_vertical_extent(rng: &mut Rng) -> VerticalExtent {
    VerticalExtent {
        staves: (0..rng.range_usize(0, 3))
            .map(|_| crate::generators::staff_id(rng))
            .collect(),
    }
}

pub fn gen_cross_region_object(rng: &mut Rng) -> CrossRegionObject {
    CrossRegionObject {
        provenance: gen_provenance(rng),
        regions: (0..rng.range_usize(1, 3))
            .map(|_| crate::generators::region_id(rng))
            .collect(),
        staff: rng.boolean().then(|| crate::generators::staff_id(rng)),
    }
}

pub fn gen_dependency_index(rng: &mut Rng) -> DependencyIndex {
    let mut index = DependencyIndex::default();
    for _ in 0..rng.range_usize(0, 4) {
        index.insert(&gen_provenance(rng));
    }
    index
}

pub fn gen_layout_cache(rng: &mut Rng) -> LayoutCache {
    LayoutCache {
        dependencies: gen_dependency_index(rng),
        logical: Default::default(),
        constrained: Default::default(),
        resolved: Default::default(),
        fine_cache: FineLayoutCache::default(),
    }
}

pub fn gen_system_id(rng: &mut Rng) -> SystemId {
    SystemId(((rng.next_u64() as u128) << 64) | rng.next_u64() as u128)
}

pub fn gen_logical_region_cache(rng: &mut Rng) -> LogicalRegionCache {
    LogicalRegionCache {
        objects: (0..rng.range_usize(0, 3))
            .map(|_| gen_layout_object_id(rng))
            .collect(),
    }
}

pub fn gen_constrained_region_cache(rng: &mut Rng) -> ConstrainedRegionCache {
    ConstrainedRegionCache {
        objects: (0..rng.range_usize(0, 3))
            .map(|_| gen_layout_object_id(rng))
            .collect(),
    }
}

pub fn gen_resolved_system_cache(rng: &mut Rng) -> ResolvedSystemCache {
    ResolvedSystemCache {
        objects: (0..rng.range_usize(0, 3))
            .map(|_| gen_layout_object_id(rng))
            .collect(),
    }
}

pub fn gen_fine_layout_cache(rng: &mut Rng) -> FineLayoutCache {
    FineLayoutCache {
        objects: (0..rng.range_usize(0, 3))
            .map(|_| gen_layout_object_id(rng))
            .collect(),
    }
}

/// A glyph-object identifier value.
pub fn gen_glyph_object_id(rng: &mut Rng) -> GlyphObjectId {
    GlyphObjectId(((rng.next_u64() as u128) << 64) | rng.next_u64() as u128)
}

pub fn gen_synthesis_instance_key(rng: &mut Rng) -> SynthesisInstanceKey {
    SynthesisInstanceKey(((rng.next_u64() as u128) << 64) | rng.next_u64() as u128)
}

pub fn gen_glyph_style(rng: &mut Rng) -> GlyphStyle {
    GlyphStyle {
        rgba: rng.next_u64() as u32,
    }
}

pub fn gen_time_point(rng: &mut Rng) -> TimePoint {
    if rng.boolean() {
        TimePoint::Musical(epiphany_core::MusicalPosition(
            epiphany_core::RationalTime::new(rng.range(0, 16) as i64, 4)
                .expect("positive denominator"),
        ))
    } else {
        TimePoint::WallClock(epiphany_core::WallClockTime(rng.next_u64() as i64))
    }
}

pub fn gen_time_range(rng: &mut Rng) -> TimeRange {
    if rng.boolean() {
        let start = epiphany_core::MusicalPosition(
            epiphany_core::RationalTime::new(rng.range(0, 8) as i64, 4)
                .expect("positive denominator"),
        );
        TimeRange::Musical {
            start: start.clone(),
            end: start
                + epiphany_core::MusicalDuration(
                    epiphany_core::RationalTime::new(1, 4).expect("positive denominator"),
                ),
        }
    } else {
        let start = epiphany_core::WallClockTime(rng.range(0, 10_000) as i64);
        TimeRange::WallClock {
            start,
            end: epiphany_core::WallClockTime(start.0 + 1_000),
        }
    }
}

pub fn gen_spring_slot(rng: &mut Rng) -> SpringSlot {
    SpringSlot {
        id: gen_spring_slot_id(rng),
        time: gen_time_point(rng),
        min_width: StaffSpace(1.0),
        preferred_width: StaffSpace(2.0),
        max_width: rng.boolean().then_some(StaffSpace(3.0)),
        stretch_factor: 1.0,
        compress_factor: 1.0,
        members: (0..rng.range_usize(0, 3))
            .map(|_| gen_glyph_object_id(rng))
            .collect(),
    }
}

pub fn gen_constrained_layout_region(rng: &mut Rng) -> ConstrainedLayoutRegion {
    let region = crate::generators::region_id(rng);
    ConstrainedLayoutRegion {
        provenance: Provenance::projected(epiphany_core::TypedObjectId::Region(region), vec![]),
        glyphs: (0..rng.range_usize(0, 3))
            .map(|_| gen_glyph_object_id(rng))
            .collect(),
        time_axis: gen_time_axis_model(rng),
    }
}

pub fn gen_layout_constraint(rng: &mut Rng) -> LayoutConstraint {
    match rng.below(6) {
        0 => LayoutConstraint::NoCollision {
            a: gen_glyph_object_id(rng),
            b: gen_glyph_object_id(rng),
        },
        1 => LayoutConstraint::Align {
            a: gen_glyph_object_id(rng),
            b: gen_glyph_object_id(rng),
            axis: if rng.boolean() {
                Axis::Horizontal
            } else {
                Axis::Vertical
            },
        },
        2 => LayoutConstraint::PositionWithin {
            glyph: gen_glyph_object_id(rng),
            region: gen_rect(rng),
        },
        3 => LayoutConstraint::SystemBreakAt {
            slot: gen_spring_slot_id(rng),
            kind: if rng.boolean() {
                BreakKind::Hard
            } else {
                BreakKind::Soft
            },
        },
        4 => LayoutConstraint::PageBreakAt {
            slot: gen_spring_slot_id(rng),
            kind: if rng.boolean() {
                BreakKind::Hard
            } else {
                BreakKind::Soft
            },
        },
        _ => LayoutConstraint::Registered(
            ConstraintRegistryId(rng.next_u64() as u128),
            ConstraintParameters(vec![rng.next_u64() as u8]),
        ),
    }
}

/// A bundled glyph metric (drawn from the in-tree Bravura table).
pub fn gen_glyph_metric(rng: &mut Rng) -> GlyphMetric {
    BRAVURA_METRICS[rng.below(BRAVURA_METRICS.len() as u64) as usize].clone()
}

/// A SMuFL version.
pub fn gen_smufl_version(rng: &mut Rng) -> SmuflVersion {
    SmuflVersion {
        major: rng.range(1, 2) as u16,
        minor: rng.range(0, 6) as u16,
    }
}

/// A font identifier (the bundled font).
pub fn gen_font_id(rng: &mut Rng) -> FontId {
    if rng.boolean() {
        FontId::BRAVURA
    } else {
        FontId::owned(format!("runtime-font-{}", rng.next_u64()))
    }
}

/// A decision source (every variant).
pub fn gen_decision_source(rng: &mut Rng) -> DecisionSource {
    match rng.below(3) {
        0 => DecisionSource::Automatic,
        1 => DecisionSource::UserOverride(EngravingOverrideId(rng.next_u64() as u128)),
        _ => DecisionSource::IrOverride,
    }
}

/// An engraving-decision kind (a representative subset).
pub fn gen_engraving_decision_kind(rng: &mut Rng) -> EngravingDecisionKind {
    match rng.below(5) {
        0 => EngravingDecisionKind::StemDirection(if rng.boolean() {
            StemDirection::Up
        } else {
            StemDirection::Down
        }),
        1 => EngravingDecisionKind::LedgerLineCount(rng.range(0, 5) as u8),
        2 => EngravingDecisionKind::SystemBreak,
        3 => EngravingDecisionKind::PageBreak,
        _ => EngravingDecisionKind::Registered(EngravingDecisionRegistryId(rng.next_u64() as u128)),
    }
}

/// An engraving-decision record with a content-derived id.
pub fn gen_engraving_decision(rng: &mut Rng) -> EngravingDecision {
    EngravingDecision::with_source(
        gen_layout_object_id(rng),
        gen_engraving_decision_kind(rng),
        gen_decision_source(rng),
    )
}

pub fn gen_override_target(rng: &mut Rng) -> OverrideTarget {
    if rng.boolean() {
        OverrideTarget::ScoreGraph(crate::generators::typed_object_id(rng))
    } else {
        OverrideTarget::IrSynthesized(gen_layout_object_id(rng))
    }
}

pub fn gen_override_kind(rng: &mut Rng) -> OverrideKind {
    match rng.below(9) {
        0 => OverrideKind::StemDirection(if rng.boolean() {
            epiphany_core::StemDirection::Up
        } else {
            epiphany_core::StemDirection::Down
        }),
        1 => OverrideKind::AccidentalParenthesized(rng.boolean()),
        2 => OverrideKind::AccidentalVisible(rng.boolean()),
        3 => OverrideKind::SystemBreak,
        4 => OverrideKind::PageBreak,
        5 => OverrideKind::HiddenObject,
        6 => OverrideKind::CustomPosition(gen_point(rng)),
        7 => OverrideKind::LedgerLineSuppression,
        _ => OverrideKind::Registered(rng.next_u64() as u128),
    }
}

pub fn gen_engraving_override(rng: &mut Rng) -> EngravingOverride {
    EngravingOverride {
        id: EngravingOverrideId(rng.next_u64() as u128),
        target: gen_override_target(rng),
        kind: gen_override_kind(rng),
        priority: if rng.boolean() {
            OverridePriority::Hard
        } else {
            OverridePriority::Soft
        },
        origin: match rng.below(4) {
            0 => OverrideOrigin::User {
                author: AuthorId(rng.next_u64() as u128),
                timestamp: Timestamp(rng.next_u64() as i64),
            },
            1 => OverrideOrigin::Import {
                format: ForeignFormatId(rng.next_u64() as u128),
            },
            2 => OverrideOrigin::Plugin {
                plugin: PluginId(rng.next_u64() as u128),
            },
            _ => OverrideOrigin::Internal,
        },
    }
}

/// A vertical-band kind (every variant).
pub fn gen_vertical_band_kind(rng: &mut Rng) -> VerticalBandKind {
    match rng.below(4) {
        0 => VerticalBandKind::Staff(crate::generators::staff_id(rng)),
        1 => VerticalBandKind::InterStaffGap,
        2 => VerticalBandKind::InterSystemGap,
        _ => VerticalBandKind::MarginBand,
    }
}

/// A vertical band (every constructor / kind exercised).
pub fn gen_vertical_band(rng: &mut Rng) -> VerticalBand {
    let members: Vec<GlyphObjectId> = (0..rng.range_usize(0, 3))
        .map(|_| gen_glyph_object_id(rng))
        .collect();
    match rng.below(4) {
        0 => VerticalBand::staff(crate::generators::staff_id(rng), members),
        1 => VerticalBand::margin(LayoutObjectId(rng.next_u64() as u128), members),
        2 => VerticalBand::inter_staff_gap(gen_vertical_band_id(rng)),
        _ => VerticalBand::inter_system_gap(gen_vertical_band_id(rng)),
    }
}

/// An operation-kind tag (every variant Agent C's type provides, including the
/// registered form).
pub fn gen_operation_kind_tag(rng: &mut Rng) -> OperationKindTag {
    match rng.below(24) {
        0 => OperationKindTag::InsertEvent,
        1 => OperationKindTag::DeleteEvent,
        2 => OperationKindTag::ModifyEvent,
        3 => OperationKindTag::RespellPitch,
        4 => OperationKindTag::Transpose,
        5 => OperationKindTag::CreateCrossCutting,
        6 => OperationKindTag::DeleteCrossCutting,
        7 => OperationKindTag::ModifyCrossCutting,
        8 => OperationKindTag::ChangeRegionTimeModel,
        9 => OperationKindTag::InsertRegion,
        10 => OperationKindTag::DeleteRegion,
        11 => OperationKindTag::InsertStaffInstance,
        12 => OperationKindTag::DeleteStaffInstance,
        13 => OperationKindTag::SetUserSystemBreak,
        14 => OperationKindTag::SetUserPageBreak,
        15 => OperationKindTag::DeclareTransaction,
        16 => OperationKindTag::InsertIdentifiedPitch,
        17 => OperationKindTag::DeleteIdentifiedPitch,
        18 => OperationKindTag::ModifyIdentifiedPitch,
        19 => OperationKindTag::CreateVoice,
        20 => OperationKindTag::DeleteVoice,
        21 => OperationKindTag::SetMetadata,
        22 => OperationKindTag::SetMetricGrid,
        _ => OperationKindTag::Registered(epiphany_ops::OperationKindRegistryId(
            rng.next_u64() as u128
        )),
    }
}

/// An object kind (the kind of a generated score-graph object).
pub fn gen_object_kind(rng: &mut Rng) -> ObjectKind {
    ObjectKind::of(&crate::generators::typed_object_id(rng))
}

/// A pitch-space id drawn from the built-in catalog.
fn gen_pitch_space_id(rng: &mut Rng) -> epiphany_core::PitchSpaceId {
    let names = ["cmn-12", "edo-31", "ji-5limit"];
    epiphany_core::PitchSpaceId::new(names[rng.below(names.len() as u64) as usize])
}

/// A barrier scope (every variant).
pub fn gen_barrier_scope(rng: &mut Rng) -> BarrierScope {
    match rng.below(8) {
        0 => BarrierScope::WholeScore,
        1 => BarrierScope::Region(crate::generators::region_id(rng)),
        2 => BarrierScope::StaffInstance(crate::generators::staff_instance_id(rng)),
        3 => BarrierScope::AnalysisLayer(crate::generators::analysis_layer_id(rng)),
        4 => BarrierScope::ObjectSet(
            (0..rng.range_usize(0, 3))
                .map(|_| crate::generators::typed_object_id(rng))
                .collect(),
        ),
        5 => BarrierScope::PitchSpace(gen_pitch_space_id(rng)),
        6 => BarrierScope::TuningContext,
        _ => BarrierScope::Registered(BarrierScopeRegistryId(rng.next_u64() as u128)),
    }
}

/// A barrier condition (bounded recursion depth; every variant).
pub fn gen_barrier_condition(rng: &mut Rng) -> BarrierCondition {
    match rng.below(7) {
        0 => BarrierCondition::Always,
        1 => BarrierCondition::ObjectExists(crate::generators::typed_object_id(rng)),
        2 => BarrierCondition::ObjectHasExtensionData {
            object: crate::generators::typed_object_id(rng),
            extension: ExtensionRef(rng.next_u64() as u128),
        },
        3 => BarrierCondition::All(vec![BarrierCondition::Always]),
        4 => BarrierCondition::Any(vec![
            BarrierCondition::Always,
            BarrierCondition::ObjectExists(crate::generators::typed_object_id(rng)),
        ]),
        5 => BarrierCondition::Not(Box::new(BarrierCondition::Always)),
        _ => BarrierCondition::Registered(BarrierConditionRegistryId(rng.next_u64() as u128)),
    }
}

/// An edit barrier keyed on operation-kind tags.
pub fn gen_edit_barrier(rng: &mut Rng) -> EditBarrier {
    EditBarrier {
        scope: gen_barrier_scope(rng),
        affected_object_kinds: (0..rng.range_usize(0, 3))
            .map(|_| gen_object_kind(rng))
            .collect(),
        prohibited_operation_kinds: (0..rng.range_usize(0, 3))
            .map(|_| gen_operation_kind_tag(rng))
            .collect(),
        condition: gen_barrier_condition(rng),
    }
}

/// An edit context (an object's structural location, every field exercised).
pub fn gen_edit_context(rng: &mut Rng) -> EditContext {
    EditContext {
        region: rng.boolean().then(|| crate::generators::region_id(rng)),
        staff_instance: rng
            .boolean()
            .then(|| crate::generators::staff_instance_id(rng)),
        analysis_layer: rng
            .boolean()
            .then(|| crate::generators::analysis_layer_id(rng)),
        pitch_space: rng.boolean().then(|| gen_pitch_space_id(rng)),
    }
}

/// A solver tier (every variant).
pub fn gen_solver_tier(rng: &mut Rng) -> SolverTier {
    *rng.choose(&[
        SolverTier::Stub,
        SolverTier::Minimal,
        SolverTier::Standard,
        SolverTier::Advanced,
    ])
}

/// A solver version.
pub fn gen_solver_version(rng: &mut Rng) -> SolverVersion {
    SolverVersion(rng.next_u64() as u32)
}

/// A deterministic solver budget.
pub fn gen_solver_budget(rng: &mut Rng) -> SolverBudget {
    SolverBudget {
        max_iterations: rng.next_u64(),
        max_nodes: rng.next_u64(),
        max_constraint_evaluations: rng.next_u64(),
        advisory_wall_time_ms: rng.boolean().then(|| rng.range(0, 1000)),
    }
}

/// A consumed-budget record.
pub fn gen_solver_budget_used(rng: &mut Rng) -> SolverBudgetUsed {
    SolverBudgetUsed {
        iterations: rng.next_u64(),
        nodes: rng.next_u64(),
        constraint_evaluations: rng.next_u64(),
        wall_time_ms: rng.next_u64(),
    }
}

/// A solver state.
pub fn gen_solver_state(rng: &mut Rng) -> SolverState {
    SolverState {
        solver_version: rng.boolean().then(|| gen_solver_version(rng)),
        resolved_glyphs: rng.range_usize(0, 32),
    }
}

/// A solver profile (every variant).
pub fn gen_solver_profile(rng: &mut Rng) -> SolverProfile {
    *rng.choose(&[
        SolverProfile::Draft,
        SolverProfile::Standard,
        SolverProfile::Publication,
    ])
}

/// Tie-breaking weights.
pub fn gen_tie_breaking_weights(rng: &mut Rng) -> TieBreakingWeights {
    let w = |r: &mut Rng| (r.range(0, 4000) as f64) / 1000.0;
    TieBreakingWeights {
        collision: w(rng),
        spacing: w(rng),
        slur_shape: w(rng),
        beam_slope: w(rng),
        vertical_density: w(rng),
        system_break: w(rng),
        page_fill: w(rng),
        casting_off: w(rng),
        symbol_density: w(rng),
    }
}

/// A solver configuration (profile + budget + tie-breaking).
pub fn gen_solver_config(rng: &mut Rng) -> SolverConfig {
    SolverConfig {
        profile: gen_solver_profile(rng),
        budget: gen_solver_budget(rng),
        tie_breaking: gen_tie_breaking_weights(rng),
    }
}

/// A normalized quality metric in `[0, 1]`.
pub fn gen_normalized_metric(rng: &mut Rng) -> NormalizedMetric {
    NormalizedMetric::new((rng.range(0, 1000) as f64) / 1000.0)
}

/// A constraint id.
pub fn gen_constraint_id(rng: &mut Rng) -> ConstraintId {
    ConstraintId(rng.next_u64() as u128)
}

/// A spring-slot id.
pub fn gen_spring_slot_id(rng: &mut Rng) -> SpringSlotId {
    SpringSlotId(rng.next_u64() as u128)
}

/// A quality-metric kind (every variant).
pub fn gen_quality_metric_kind(rng: &mut Rng) -> QualityMetricKind {
    *rng.choose(&[
        QualityMetricKind::Collision,
        QualityMetricKind::Spacing,
        QualityMetricKind::SlurShape,
        QualityMetricKind::BeamSlope,
        QualityMetricKind::VerticalDensity,
        QualityMetricKind::SystemBreak,
        QualityMetricKind::PageFill,
        QualityMetricKind::CastingOff,
        QualityMetricKind::SymbolDensity,
    ])
}

/// An extension-warning id.
pub fn gen_extension_warning_id(rng: &mut Rng) -> ExtensionWarningId {
    ExtensionWarningId(rng.next_u64() as u128)
}

/// A solver warning (every kind).
pub fn gen_solver_warning(rng: &mut Rng) -> SolverWarning {
    let kind = match rng.below(4) {
        0 => SolverWarningKind::LargeSoftConstraintViolation {
            constraint: gen_constraint_id(rng),
            magnitude: (rng.range(0, 10_000) as f64) / 1000.0,
        },
        1 => SolverWarningKind::UnusualLayoutDecision("stub".to_string()),
        2 => SolverWarningKind::QualityFloorApproached {
            metric: gen_quality_metric_kind(rng),
        },
        _ => SolverWarningKind::ExtensionWarning(gen_extension_warning_id(rng)),
    };
    SolverWarning {
        kind,
        affected_objects: (0..rng.range_usize(0, 2))
            .map(|_| crate::generators::typed_object_id(rng))
            .collect(),
        message: "generated".to_string(),
    }
}

/// An extension-metric id.
pub fn gen_extension_metric_id(rng: &mut Rng) -> ExtensionMetricId {
    ExtensionMetricId(rng.next_u64() as u128)
}

/// An extension-contributed quality metric.
pub fn gen_extension_metric(rng: &mut Rng) -> ExtensionMetric {
    ExtensionMetric {
        metric_id: gen_extension_metric_id(rng),
        value: gen_normalized_metric(rng),
    }
}

/// A (non-default) quality metric vector.
pub fn gen_quality_metric_vector(rng: &mut Rng) -> QualityMetricVector {
    QualityMetricVector {
        collision_penalty: gen_normalized_metric(rng),
        spacing_distortion: gen_normalized_metric(rng),
        slur_shape_penalty: gen_normalized_metric(rng),
        beam_slope_penalty: gen_normalized_metric(rng),
        vertical_density_penalty: gen_normalized_metric(rng),
        system_break_penalty: gen_normalized_metric(rng),
        page_fill_efficiency: gen_normalized_metric(rng),
        casting_off_quality: gen_normalized_metric(rng),
        symbol_density_uniformity: gen_normalized_metric(rng),
        extension_metrics: (0..rng.range_usize(0, 2))
            .map(|_| gen_extension_metric(rng))
            .collect(),
    }
}

/// A render target (every variant).
pub fn gen_render_target(rng: &mut Rng) -> RenderTarget {
    *rng.choose(&[
        RenderTarget::Pdf,
        RenderTarget::Svg,
        RenderTarget::Screen,
        RenderTarget::Print,
    ])
}

pub fn gen_color_configuration(rng: &mut Rng) -> ColorConfiguration {
    ColorConfiguration {
        color_space: *rng.choose(&[
            ColorSpace::Srgb,
            ColorSpace::DisplayP3,
            ColorSpace::Cmyk,
            ColorSpace::Grayscale,
        ]),
        embed_profile: rng.boolean(),
    }
}

pub fn gen_rasterization_configuration(rng: &mut Rng) -> RasterizationConfiguration {
    RasterizationConfiguration {
        antialias: rng.boolean(),
        dpi: rng.range(72, 1201) as u32,
    }
}

/// A render configuration.
pub fn gen_render_configuration(rng: &mut Rng) -> RenderConfiguration {
    RenderConfiguration {
        target: gen_render_target(rng),
        color: gen_color_configuration(rng),
        rasterization: gen_rasterization_configuration(rng),
    }
}

/// An invalidation scope (every variant).
pub fn gen_invalidation_scope(rng: &mut Rng) -> InvalidationScope {
    *rng.choose(&[
        InvalidationScope::ObjectLocal,
        InvalidationScope::MeasureLocal,
        InvalidationScope::SystemLocal,
        InvalidationScope::PageLocal,
        InvalidationScope::RegionLocal,
        InvalidationScope::WholeScore,
    ])
}

/// An invalidation set over generated slots, bands, constraints, and glyphs.
pub fn gen_invalidation_set(rng: &mut Rng) -> InvalidationSet {
    let n = |r: &mut Rng| r.range_usize(0, 3);
    InvalidationSet {
        scope: gen_invalidation_scope(rng),
        slots: (0..n(rng)).map(|_| gen_spring_slot_id(rng)).collect(),
        bands: (0..n(rng)).map(|_| gen_vertical_band_id(rng)).collect(),
        constraints: (0..n(rng)).map(|_| gen_constraint_id(rng)).collect(),
        glyphs: (0..n(rng)).map(|_| gen_glyph_object_id(rng)).collect(),
    }
}

// --- Remaining public-type generators (Agent F mandate completeness) ---

/// A render-scale context.
pub fn gen_scale_context(rng: &mut Rng) -> ScaleContext {
    ScaleContext {
        points_per_staff_space: 4.0 + (rng.range(0, 6000) as f32) / 1000.0,
    }
}

/// A glyph anchor.
pub fn gen_glyph_anchor(rng: &mut Rng) -> GlyphAnchor {
    let names = ["stemUpNW", "stemDownSE", "cutOutNE"];
    GlyphAnchor {
        name: std::borrow::Cow::Borrowed(names[rng.below(names.len() as u64) as usize]),
        x: rng.range(0, 2048) as i32,
        y: rng.range(0, 2048) as i32,
    }
}

/// A vertical-band id.
pub fn gen_vertical_band_id(rng: &mut Rng) -> VerticalBandId {
    VerticalBandId(((rng.next_u64() as u128) << 64) | rng.next_u64() as u128)
}

/// A time-axis kind (every variant).
pub fn gen_time_axis_kind(rng: &mut Rng) -> TimeAxisKind {
    match rng.below(4) {
        0 => TimeAxisKind::Metric,
        1 => TimeAxisKind::Proportional,
        2 => TimeAxisKind::Aleatoric,
        _ => TimeAxisKind::Registered(TimeAxisRegistryId(rng.next_u64() as u128)),
    }
}

/// An engraving-decision id (content-derived from a generated decision).
pub fn gen_engraving_decision_id(rng: &mut Rng) -> EngravingDecisionId {
    gen_engraving_decision(rng).id
}

/// A Sem-ver font version.
pub fn gen_sem_ver(rng: &mut Rng) -> SemVer {
    SemVer::new(
        rng.range(0, 4) as u32,
        rng.range(0, 400) as u32,
        rng.range(0, 9) as u32,
    )
}

/// Glyph render data availability.
pub fn gen_glyph_render_data(rng: &mut Rng) -> GlyphRenderData {
    GlyphRenderData {
        outline: if rng.boolean() {
            vec![gen_path_command(rng), PathCommand::Close]
        } else {
            vec![]
        },
        bitmap: rng.boolean().then(|| gen_glyph_bitmap(rng)),
    }
}

pub fn gen_path_command(rng: &mut Rng) -> PathCommand {
    match rng.below(4) {
        0 => PathCommand::MoveTo(gen_point(rng)),
        1 => PathCommand::LineTo(gen_point(rng)),
        2 => PathCommand::CurveTo {
            control1: gen_point(rng),
            control2: gen_point(rng),
            to: gen_point(rng),
        },
        _ => PathCommand::Close,
    }
}

pub fn gen_glyph_bitmap(rng: &mut Rng) -> GlyphBitmap {
    GlyphBitmap {
        width: 1,
        height: 1,
        rgba8: vec![rng.next_u64() as u8; 4],
    }
}

/// A staff-space bounding box drawn from a bundled glyph.
pub fn gen_glyph_bounding_box(rng: &mut Rng) -> BoundingBox {
    gen_glyph_metric(rng).bounding_box()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use epiphany_core::TypedObjectId;
    use epiphany_determinism::CanonicalEncode;

    #[test]
    fn ir_generators_are_deterministic_and_well_formed() {
        let mut a = Rng::new(13);
        let mut b = Rng::new(13);
        // Deterministic from the seed.
        assert_eq!(gen_logical_layout_ir(&mut a), gen_logical_layout_ir(&mut b));
        let generated = gen_logical_layout_ir(&mut Rng::new(14));
        let constrained = try_to_constrained(&generated)
            .expect("generated logical IR must be structurally transformable");
        assert_eq!(
            StubSolver
                .solve(&constrained, &SolverConfig::default())
                .status,
            SolveStatus::Solved
        );
        // Provenance ids are consistent with their synthesis: a projected one
        // (no synthesis) carries the source-only id; a synthesized one does not.
        // Every generated constrained IR is accepted by the real stub solver.
        let mut rng = Rng::new(99);
        for _ in 0..64 {
            let p = gen_provenance(&mut rng);
            match p.synthesis {
                None => assert_eq!(p.stable_id, stable_layout_id(&p.source)),
                Some(_) => assert_ne!(p.stable_id, stable_layout_id(&p.source)),
            }
            let ci = gen_constrained_layout_ir(&mut rng);
            assert_eq!(
                StubSolver.solve(&ci, &SolverConfig::default()).status,
                SolveStatus::Solved
            );
            let _ = gen_glyph_object(&mut rng);
            let _ = gen_resolved_glyph(&mut rng);
            let _ = gen_time_axis_model(&mut rng);
            let _ = gen_synthesis_kind(&mut rng);
            let _ = gen_solve_status(&mut rng);
            let _ = gen_glyph_catalog_identity(&mut rng);
            let _ = gen_layout_object_id(&mut rng);
            let _ = gen_resolved_layout_ir(&mut rng);
            let _ = gen_render_primitive(&mut rng);
            let _ = gen_render_ir(&mut rng);
            let _ = gen_solve_report(&mut rng);
            let _ = gen_round_trip_report(&mut rng);
            // The rest of E's public surface.
            let _ = gen_staff_space(&mut rng);
            let _ = gen_size2d(&mut rng);
            let _ = gen_bounding_box(&mut rng);
            let _ = gen_glyph_object_id(&mut rng);
            let _ = gen_glyph_metric(&mut rng);
            let _ = gen_smufl_version(&mut rng);
            let _ = gen_font_id(&mut rng);
            let _ = gen_engraving_decision(&mut rng);
            let _ = gen_vertical_band(&mut rng);
            let _ = gen_vertical_band_kind(&mut rng);
            let barrier = gen_edit_barrier(&mut rng);
            // The barrier canonically encodes (Appendix D), and prohibition is
            // well-defined against a generated edit context.
            let _ = barrier.to_canonical_bytes();
            let _ = barrier.prohibits_edit(
                gen_operation_kind_tag(&mut rng),
                &crate::generators::typed_object_id(&mut rng),
                &gen_edit_context(&mut rng),
                &AlwaysLiveOracle,
            );
            let _ = gen_solver_config(&mut rng);
            let _ = gen_solver_tier(&mut rng);
            let _ = gen_solver_version(&mut rng);
            let _ = gen_invalidation_set(&mut rng);
            // The remaining public surface.
            let _ = gen_solver_budget_used(&mut rng);
            let _ = gen_solver_state(&mut rng);
            let _ = gen_solver_profile(&mut rng);
            let _ = gen_tie_breaking_weights(&mut rng);
            let _ = gen_normalized_metric(&mut rng);
            let _ = gen_constraint_id(&mut rng);
            let _ = gen_spring_slot_id(&mut rng);
            let _ = gen_solver_warning(&mut rng);
            let _ = gen_scale_context(&mut rng);
            let _ = gen_glyph_anchor(&mut rng);
            let _ = gen_vertical_band_id(&mut rng);
            let _ = gen_time_axis_kind(&mut rng);
            let _ = gen_engraving_decision_id(&mut rng);
            let _ = gen_sem_ver(&mut rng);
            let _ = gen_glyph_render_data(&mut rng);
            let _ = gen_glyph_bounding_box(&mut rng);
            let _ = gen_decision_source(&mut rng);
            let _ = gen_engraving_decision_kind(&mut rng);
            let _ = gen_object_kind(&mut rng);
            let _ = gen_barrier_scope(&mut rng);
            let _ = gen_barrier_condition(&mut rng);
            let _ = gen_invalidation_scope(&mut rng);
            let _ = gen_size2d(&mut rng);
            let _ = gen_layout_object(&mut rng);
            let _ = gen_layout_region(&mut rng);
            let _ = gen_font_id(&mut rng);
            let _ = gen_smufl_version(&mut rng);
            let _ = gen_quality_metric_kind(&mut rng);
            let _ = gen_extension_warning_id(&mut rng);
            let _ = gen_extension_metric(&mut rng);
            let _ = gen_extension_metric_id(&mut rng);
            let _ = gen_quality_metric_vector(&mut rng);
            let _ = gen_render_target(&mut rng);
            let _ = gen_render_configuration(&mut rng);
            let _ = gen_glyph_render_data(&mut rng);
            let _ = gen_vertical_band(&mut rng);
            let _ = gen_rect(&mut rng);
            let _ = gen_transform_2d(&mut rng);
            let _ = gen_margins(&mut rng);
            let _ = gen_score_version(&mut rng);
            let _ = gen_local_coordinate_system(&mut rng);
            let _ = gen_vertical_extent(&mut rng);
            let _ = gen_cross_region_object(&mut rng);
            let _ = gen_synthesis_instance_key(&mut rng);
            let _ = gen_glyph_style(&mut rng);
            let _ = gen_time_point(&mut rng);
            let _ = gen_time_range(&mut rng);
            let _ = gen_spring_slot(&mut rng);
            let _ = gen_constrained_layout_region(&mut rng);
            let _ = gen_layout_constraint(&mut rng);
            let _ = gen_engraving_override(&mut rng);
            let _ = gen_resolved_staff(&mut rng);
            let _ = gen_resolved_measure(&mut rng);
            let _ = gen_resolved_system(&mut rng);
            let _ = gen_resolved_page(&mut rng);
            let _ = gen_dependency_index(&mut rng);
            let _ = gen_layout_cache(&mut rng);
            let _ = gen_system_id(&mut rng);
            let _ = gen_logical_region_cache(&mut rng);
            let _ = gen_constrained_region_cache(&mut rng);
            let _ = gen_resolved_system_cache(&mut rng);
            let _ = gen_fine_layout_cache(&mut rng);
            let _ = gen_color_configuration(&mut rng);
            let _ = gen_rasterization_configuration(&mut rng);
            let _ = gen_path_command(&mut rng);
            let _ = gen_glyph_bitmap(&mut rng);
        }
    }

    #[test]
    fn solver_rejects_a_catalog_hash_that_misses_its_glyphs() {
        let mut rng = Rng::new(8);
        let mut input = gen_constrained_layout_ir(&mut rng);
        // Ensure there is at least one glyph to consult.
        if input.glyphs.is_empty() {
            input = gen_constrained_layout_ir(&mut Rng::new(3));
        }
        assert_eq!(
            StubSolver.solve(&input, &SolverConfig::default()).status,
            SolveStatus::Solved
        );
        input.catalog.metrics_hash[0] ^= 1;
        assert_eq!(
            StubSolver.solve(&input, &SolverConfig::default()).status,
            SolveStatus::InternalError,
            "solver must reject a catalog hash that does not cover consulted metrics"
        );
    }

    #[test]
    fn ten_measure_single_staff_round_trips() {
        // The QUICKSTART's headline case for Agent E's hand-off: a real
        // 10-measure single-staff score, driven through the real crate.
        let score = fixtures::ten_measure_single_staff(0xA11CE);
        let report = round_trip(&score);
        assert!(report.glyphs > 0);
        assert_eq!(report.glyphs, report.render_primitives);

        let measures = report
            .recovered_sources
            .iter()
            .filter(|s| matches!(s, TypedObjectId::Measure(_)))
            .count();
        assert_eq!(measures, 10, "all ten measures must be laid out");
        assert!(report
            .recovered_sources
            .iter()
            .any(|s| matches!(s, TypedObjectId::Tie(_))));
        assert!(report
            .recovered_sources
            .iter()
            .any(|s| matches!(s, TypedObjectId::ChordSymbol(_))));
    }
}
