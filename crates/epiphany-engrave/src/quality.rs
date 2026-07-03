//! **Real quality-metric computation** — the nine normative axes of the
//! *Quality Metric Catalog* companion (v0.1.0, Chapter 3), measured over what
//! the pipeline already produced: the resolved world-frame geometry
//! ([`CastLayout`]), the constrained input (slot identity, vertical bands), and
//! the declared page geometry. Normalization anchors, threshold tables, and the
//! warning fraction are the catalog's, transcribed in
//! [`epiphany_layout_ir::quality`].
//!
//! Every measurement here is a pure function of the solve's inputs and its
//! resolved output — no clocks, no entropy, fixed iteration order — so repeated
//! identical solves yield bitwise-identical vectors (catalog
//! `req:qmc:determinism`). Where a metric's contributing-unit set is empty the
//! axis is exactly `0.0` (the catalog's vacuous-geometry rule,
//! `req:qmc:vacuous`), never a sentinel.
//!
//! ## Where each axis's inputs come from
//!
//! * **`collision_penalty`** — full pairwise same-system sweep over resolved
//!   glyph ink boxes (positions from casting, boxes from the catalog metrics),
//!   excluding same-slot pairs (slot identity = the glyph's
//!   `horizontal_slot` in the constrained input; the resolved glyph list is
//!   index-parallel to it) and strokes (not glyphs, never swept).
//! * **`spacing_distortion`** — per-system column advances: the distinct
//!   resolved x of each glyph-bearing slot realized in the system (its first
//!   member's baseline — the spacing pass's own column reference).
//! * **`slur_shape_penalty` / `beam_slope_penalty`** — **vacuous 0.0**: the
//!   pipeline draws no slur or beam geometry (slurs/beams exist logically,
//!   not as curves/segments), so the contributing-unit sets are empty. The
//!   catalog pins vacuous-0.0 deliberately and owns the honesty edge (its
//!   "notated-but-unrendered" open question): rendering completeness is
//!   governed by constraint families and visual acceptance, not these axes.
//! * **`vertical_density_penalty`** — realized gaps against the band model's
//!   preferred heights: the constrained input's `InterStaffGap` bands
//!   (adjacent staff bands' resolved ink extents; the constrained stage's
//!   fixed staff stacking is preserved verbatim, so this measures what the
//!   resolved geometry actually shows), plus the casting pass's realized
//!   inter-system gaps (consecutive systems on a page) against
//!   [`VerticalBand::inter_system_gap`]'s preferred height — the same
//!   constructor the stacking consults. (`to_constrained` declares no
//!   `InterSystemGap` bands, so the realized page-tree gaps are the honest
//!   measurable unit set; see DECISIONS.)
//! * **`system_break_penalty`** — per-region non-final systems: `|W − w_s| / W`
//!   with `W` the declared content width and `w_s` the system's glyph-ink
//!   span.
//! * **`page_fill_efficiency`** — non-final pages: unfilled fraction of the
//!   declared content height, spans from the resolved page tree's system
//!   bounding boxes (top of first system to bottom of last).
//! * **`casting_off_quality`** — per-region CV of system glyph-ink widths,
//!   final system included (regions with ≥ 2 systems, all widths positive).
//! * **`symbol_density_uniformity`** — per-region CV of glyphs-per-width
//!   density over systems with positive width.

use std::collections::{BTreeMap, BTreeSet};

use epiphany_layout_ir::quality::{
    anchors, normalize, MetricThresholds, QUALITY_FLOOR_FRACTION, QUALITY_METRIC_KINDS,
};
use epiphany_layout_ir::{
    inter_staff_gap_id, ConstrainedLayoutIR, GlyphObject, GlyphObjectId, QualityMetricVector,
    SolverWarning, SolverWarningKind, SpringSlotId, VerticalBand, VerticalBandId, VerticalBandKind,
};

use crate::casting::{CastLayout, PageGeometry};

/// The population coefficient of variation (catalog §"The Measurement Domain"):
/// defined for `k >= 2` values with positive mean; `None` otherwise.
fn cv(values: &[f64]) -> Option<f64> {
    if values.len() < 2 {
        return None;
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    if mean <= 0.0 {
        return None;
    }
    let variance =
        values.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / values.len() as f64;
    Some(variance.sqrt() / mean)
}

/// The arithmetic mean over a contributing-unit set, with the catalog's
/// vacuous-geometry rule in aggregate form: the mean over an empty set is `0`.
fn mean_or_zero(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

/// One glyph's resolved ink box `[left, bottom, right, top]` (f64, exact from
/// the f32 geometry).
fn ink_box(cast: &CastLayout, input: &ConstrainedLayoutIR, index: usize) -> [f64; 4] {
    let resolved = &cast.glyphs[index];
    let bounds = &input.glyphs[index].bounding_box;
    [
        f64::from(resolved.position.x.0 + bounds.left.0),
        f64::from(resolved.position.y.0 + bounds.bottom.0),
        f64::from(resolved.position.x.0 + bounds.right.0),
        f64::from(resolved.position.y.0 + bounds.top.0),
    ]
}

/// Per-system aggregates over the casting pass's own glyph→system assignment.
struct SystemCensus {
    /// Region each system slices (parallel to the other vectors).
    region: Vec<usize>,
    /// Glyph indices per system, in input order.
    members: Vec<Vec<usize>>,
    /// Glyph-ink span `w_s` per system (0 for a glyph-less system).
    width: Vec<f64>,
    /// Column reference x per realized slot per system, ascending and distinct.
    columns: Vec<Vec<f64>>,
}

fn census(input: &ConstrainedLayoutIR, cast: &CastLayout) -> SystemCensus {
    let count = cast.region_of_system.len();
    let mut members: Vec<Vec<usize>> = vec![Vec::new(); count];
    let mut spans: Vec<Option<(f64, f64)>> = vec![None; count];
    // Column reference: the slot's first member (input order) — the same
    // convention the spacing and casting passes use for a slot's reference x.
    let mut columns: Vec<BTreeMap<SpringSlotId, f64>> = vec![BTreeMap::new(); count];
    for (index, glyph) in input.glyphs.iter().enumerate() {
        let Some(&system) = cast.system_of_slot.get(&glyph.horizontal_slot) else {
            // A slot no region claimed: positioned by no system, so its glyphs
            // join no per-system aggregate (catalog §"The Measurement Domain").
            continue;
        };
        members[system].push(index);
        let [left, _, right, _] = ink_box(cast, input, index);
        spans[system] = Some(match spans[system] {
            Some((lo, hi)) => (lo.min(left), hi.max(right)),
            None => (left, right),
        });
        columns[system]
            .entry(glyph.horizontal_slot)
            .or_insert_with(|| f64::from(cast.glyphs[index].position.x.0));
    }
    let width = spans
        .iter()
        .map(|span| span.map(|(lo, hi)| (hi - lo).max(0.0)).unwrap_or(0.0))
        .collect();
    let columns = columns
        .into_iter()
        .map(|by_slot| {
            let mut xs: Vec<f64> = by_slot.into_values().collect();
            xs.sort_by(f64::total_cmp);
            xs.dedup();
            xs
        })
        .collect();
    SystemCensus {
        region: cast.region_of_system.clone(),
        members,
        width,
        columns,
    }
}

/// `collision_penalty` (catalog §`collision_penalty`): colliding unordered
/// same-system, different-slot glyph pairs per glyph. Ink boxes must intersect
/// with positive area in both axes; edge-touching boxes do not collide;
/// same-slot pairs (a column's internal cluster — chord heads, their
/// accidentals, dots) are excluded; strokes are not glyphs and join no pair.
fn collision_raw(input: &ConstrainedLayoutIR, cast: &CastLayout, census: &SystemCensus) -> f64 {
    let population = cast.glyphs.len();
    if population == 0 {
        return 0.0;
    }
    let mut colliding_pairs: u64 = 0;
    for members in &census.members {
        // Interval sweep over left edges: a pair can only overlap horizontally
        // while the candidate's left edge is inside the anchor's span.
        let mut boxes: Vec<(usize, [f64; 4])> = members
            .iter()
            .map(|&index| (index, ink_box(cast, input, index)))
            .collect();
        boxes.sort_by(|a, b| a.1[0].total_cmp(&b.1[0]).then(a.0.cmp(&b.0)));
        for i in 0..boxes.len() {
            let (index_a, a) = boxes[i];
            for &(index_b, b) in boxes.iter().skip(i + 1) {
                if b[0] >= a[2] {
                    break; // sorted by left edge: nothing further overlaps in x
                }
                if input.glyphs[index_a].horizontal_slot == input.glyphs[index_b].horizontal_slot {
                    continue; // same-column cluster: excluded by the catalog
                }
                let overlap_x = a[2].min(b[2]) - a[0].max(b[0]);
                let overlap_y = a[3].min(b[3]) - a[1].max(b[1]);
                if overlap_x > 0.0 && overlap_y > 0.0 {
                    colliding_pairs += 1;
                }
            }
        }
    }
    colliding_pairs as f64 / population as f64
}

/// `spacing_distortion` (catalog §`spacing_distortion`): mean per-system CV of
/// column advances, over systems realizing at least three columns.
fn spacing_raw(census: &SystemCensus) -> f64 {
    let mut per_system = Vec::new();
    for columns in &census.columns {
        if columns.len() < 3 {
            continue;
        }
        let advances: Vec<f64> = columns.windows(2).map(|pair| pair[1] - pair[0]).collect();
        if let Some(value) = cv(&advances) {
            per_system.push(value);
        }
    }
    mean_or_zero(&per_system)
}

/// `vertical_density_penalty` (catalog §`vertical_density_penalty`): mean
/// relative deviation `|r − p| / p` over the realized inter-staff and
/// inter-system gaps (see the module docs for the unit reconstruction).
fn vertical_raw(input: &ConstrainedLayoutIR, cast: &CastLayout, census: &SystemCensus) -> f64 {
    let mut per_unit: Vec<f64> = Vec::new();

    // --- InterStaffGap bands declared by the constrained input -------------
    let index_of: BTreeMap<GlyphObjectId, usize> = input
        .glyphs
        .iter()
        .enumerate()
        .map(|(index, glyph)| (GlyphObject::id(glyph), index))
        .collect();
    let system_of_glyph = |index: usize| -> Option<usize> {
        cast.system_of_slot
            .get(&input.glyphs[index].horizontal_slot)
            .copied()
    };
    for (region_index, region) in input.regions.iter().enumerate() {
        // The region's laid-out staff bands, top staff first, ordered within
        // the region's first system (systems translate rigidly, so within-
        // system y order is the region's staff order).
        let first_system = census.region.iter().position(|&r| r == region_index);
        let Some(first_system) = first_system else {
            continue;
        };
        let region_glyphs: BTreeSet<GlyphObjectId> = region.glyphs.iter().copied().collect();
        let mut staves: Vec<(f64, Vec<usize>)> = Vec::new();
        for band in &input.vertical_bands {
            if !matches!(band.kind, VerticalBandKind::Staff(_)) {
                continue;
            }
            if !band.members.iter().any(|id| region_glyphs.contains(id)) {
                continue;
            }
            let members: Vec<usize> = band
                .members
                .iter()
                .filter_map(|id| index_of.get(id).copied())
                .collect();
            let top_in_first = members
                .iter()
                .filter(|&&index| system_of_glyph(index) == Some(first_system))
                .map(|&index| ink_box(cast, input, index)[3])
                .fold(f64::NEG_INFINITY, f64::max);
            if top_in_first.is_finite() {
                staves.push((top_in_first, members));
            }
        }
        // Top staff first.
        staves.sort_by(|a, b| b.0.total_cmp(&a.0));

        // The region's declared inter-staff gap bands, by their derived ids
        // (gap g separates the region's staves g−1 and g, per to_constrained).
        let region_layout_id = region.provenance.stable_id;
        for gap in 1.. {
            let gap_id = inter_staff_gap_id(region_layout_id, gap);
            let Some(band) = input.vertical_bands.iter().find(|band| band.id == gap_id) else {
                break;
            };
            let preferred = f64::from(band.preferred_height.0);
            if preferred <= 0.0 || staves.len() <= gap {
                continue;
            }
            let upper = &staves[gap - 1].1;
            let lower = &staves[gap].1;
            // Realized iff the adjacent content shares a system; measure the
            // separation there (rigid system translation makes every common
            // system agree).
            let common: BTreeSet<usize> = upper
                .iter()
                .filter_map(|&index| system_of_glyph(index))
                .filter(|system| {
                    lower
                        .iter()
                        .any(|&index| system_of_glyph(index) == Some(*system))
                })
                .collect();
            let Some(&system) = common.iter().next() else {
                continue;
            };
            let upper_bottom = upper
                .iter()
                .filter(|&&index| system_of_glyph(index) == Some(system))
                .map(|&index| ink_box(cast, input, index)[1])
                .fold(f64::INFINITY, f64::min);
            let lower_top = lower
                .iter()
                .filter(|&&index| system_of_glyph(index) == Some(system))
                .map(|&index| ink_box(cast, input, index)[3])
                .fold(f64::NEG_INFINITY, f64::max);
            let realized = (upper_bottom - lower_top).max(0.0);
            per_unit.push((realized - preferred).abs() / preferred);
        }
    }

    // --- Realized inter-system gaps (consecutive systems on a page) --------
    let preferred = f64::from(
        VerticalBand::inter_system_gap(VerticalBandId(0))
            .preferred_height
            .0,
    );
    if preferred > 0.0 {
        for page in &cast.pages {
            for pair in page.systems.windows(2) {
                let upper_bottom = f64::from(pair[0].bounding_box.origin.y.0);
                let lower_top =
                    f64::from(pair[1].bounding_box.origin.y.0 + pair[1].bounding_box.size.height.0);
                let realized = (upper_bottom - lower_top).max(0.0);
                per_unit.push((realized - preferred).abs() / preferred);
            }
        }
    }

    mean_or_zero(&per_unit)
}

/// `system_break_penalty` (catalog §`system_break_penalty`): mean
/// `|W − w_s| / W` over each region's non-final systems, defined only for a
/// finite positive content width.
fn system_break_raw(census: &SystemCensus, content_width: f64) -> f64 {
    if !(content_width.is_finite() && content_width > 0.0) {
        return 0.0;
    }
    let mut per_unit = Vec::new();
    for (system, &region) in census.region.iter().enumerate() {
        let last_of_region = census.region.iter().rposition(|&r| r == region);
        if last_of_region == Some(system) {
            continue; // a short last line is not a break failure
        }
        per_unit.push((content_width - census.width[system]).abs() / content_width);
    }
    mean_or_zero(&per_unit)
}

/// `page_fill_efficiency` (catalog §`page_fill_efficiency`): mean unfilled
/// fraction over non-final pages, spans measured from the resolved page tree
/// (top of the first system's content extent to the bottom of the last's).
fn page_fill_raw(cast: &CastLayout, content_height: f64) -> f64 {
    if !(content_height.is_finite() && content_height > 0.0) || cast.pages.len() < 2 {
        return 0.0;
    }
    let mut per_unit = Vec::new();
    for page in &cast.pages[..cast.pages.len() - 1] {
        let (Some(first), Some(last)) = (page.systems.first(), page.systems.last()) else {
            continue;
        };
        let top = f64::from(first.bounding_box.origin.y.0 + first.bounding_box.size.height.0);
        let bottom = f64::from(last.bounding_box.origin.y.0);
        let fill = ((top - bottom) / content_height).min(1.0);
        per_unit.push(1.0 - fill);
    }
    mean_or_zero(&per_unit)
}

/// `casting_off_quality` (catalog §`casting_off_quality`): mean per-region CV
/// of system widths — final system included — over regions cast onto at least
/// two systems, each with positive width.
fn casting_off_raw(input: &ConstrainedLayoutIR, census: &SystemCensus) -> f64 {
    let mut per_region = Vec::new();
    for region in 0..input.regions.len() {
        let widths: Vec<f64> = census
            .region
            .iter()
            .zip(&census.width)
            .filter(|&(&r, _)| r == region)
            .map(|(_, &w)| w)
            .collect();
        if widths.len() < 2 || widths.iter().any(|&w| w <= 0.0) {
            continue;
        }
        if let Some(value) = cv(&widths) {
            per_region.push(value);
        }
    }
    mean_or_zero(&per_region)
}

/// `symbol_density_uniformity` (catalog §`symbol_density_uniformity`): mean
/// per-region CV of per-system symbol density (glyphs per staff space of
/// content width), over regions with at least two positive-width systems.
fn symbol_density_raw(input: &ConstrainedLayoutIR, census: &SystemCensus) -> f64 {
    let mut per_region = Vec::new();
    for region in 0..input.regions.len() {
        let densities: Vec<f64> = census
            .region
            .iter()
            .enumerate()
            .filter(|&(system, &r)| r == region && census.width[system] > 0.0)
            .map(|(system, _)| census.members[system].len() as f64 / census.width[system])
            .collect();
        if densities.len() < 2 {
            continue;
        }
        if let Some(value) = cv(&densities) {
            per_region.push(value);
        }
    }
    mean_or_zero(&per_region)
}

/// Computes the full nine-axis [`QualityMetricVector`] for a cast layout, per
/// the Quality Metric Catalog's formulas and pinned anchors. Pure and
/// deterministic: a function of the constrained input, the cast output, and
/// the declared page geometry.
pub(crate) fn measure(
    input: &ConstrainedLayoutIR,
    cast: &CastLayout,
    geometry: &PageGeometry,
) -> QualityMetricVector {
    let census = census(input, cast);
    let content_width = f64::from(geometry.content_width());
    let content_height = f64::from(geometry.content_height());
    QualityMetricVector {
        collision_penalty: normalize(
            collision_raw(input, cast, &census),
            anchors::COLLISION_R_WORST,
        ),
        spacing_distortion: normalize(spacing_raw(&census), anchors::SPACING_R_WORST),
        // No drawn slur geometry exists in this pipeline (slurs are logical
        // objects, not curves): the contributing-unit set is empty, so the
        // axis is exactly 0.0 per the catalog's vacuous-geometry rule. The
        // catalog's "notated-but-unrendered" open question owns the honesty
        // edge; the definition is pinned so the first slur-drawing release is
        // measured from day one.
        slur_shape_penalty: normalize(0.0, anchors::SLUR_SHAPE_R_WORST),
        // Same vacuous rule: no drawn beam segments exist in this pipeline.
        beam_slope_penalty: normalize(0.0, anchors::BEAM_SLOPE_R_WORST),
        vertical_density_penalty: normalize(
            vertical_raw(input, cast, &census),
            anchors::VERTICAL_DENSITY_R_WORST,
        ),
        system_break_penalty: normalize(
            system_break_raw(&census, content_width),
            anchors::SYSTEM_BREAK_R_WORST,
        ),
        page_fill_efficiency: normalize(
            page_fill_raw(cast, content_height),
            anchors::PAGE_FILL_R_WORST,
        ),
        casting_off_quality: normalize(
            casting_off_raw(input, &census),
            anchors::CASTING_OFF_R_WORST,
        ),
        symbol_density_uniformity: normalize(
            symbol_density_raw(input, &census),
            anchors::SYMBOL_DENSITY_R_WORST,
        ),
        extension_metrics: Vec::new(),
    }
}

/// The `QualityFloorApproached` warnings a computed vector earns (catalog
/// §"The `QualityFloorApproached` Warning", `req:qmc:floor-warning`): one per
/// axis whose value exceeds [`QUALITY_FLOOR_FRACTION`] × the applicable
/// threshold — the column selected by the solve's profile. The warning is
/// diagnostic; per the catalog it does **not** change the solve's status.
pub(crate) fn floor_warnings(
    vector: &QualityMetricVector,
    thresholds: &MetricThresholds,
) -> Vec<SolverWarning> {
    QUALITY_METRIC_KINDS
        .iter()
        .filter_map(|&kind| {
            let value = vector.axis(kind).0;
            let threshold = thresholds.axis(kind);
            let floor = QUALITY_FLOOR_FRACTION * threshold;
            (value > floor).then(|| SolverWarning {
                kind: SolverWarningKind::QualityFloorApproached { metric: kind },
                affected_objects: Vec::new(),
                message: format!(
                    "quality metric {kind:?} at {value:.4} exceeds {QUALITY_FLOOR_FRACTION} x \
                     the profile's threshold {threshold:.2} (floor {floor:.3})"
                ),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::Engraver;
    use epiphany_layout_ir::{
        to_constrained, to_logical, ConstrainedLayoutIR, ConstraintSolver, QualityMetricKind,
        QualityMetricVector, SolveStatus, SolverConfig, SolverProfile, SolverWarningKind,
        QUALITY_METRIC_KINDS,
    };

    /// The QUICKSTART ten-measure hand-off fixture: wraps into two systems
    /// under the default A4 geometry — the multi-system measurement case.
    fn ten_measure() -> ConstrainedLayoutIR {
        to_constrained(&to_logical(
            &epiphany_testkit::fixtures::ten_measure_single_staff(0x000A_11CE),
        ))
    }

    fn axes(vector: &QualityMetricVector) -> [f64; 9] {
        let mut values = [0.0; 9];
        for (slot, kind) in values.iter_mut().zip(QUALITY_METRIC_KINDS) {
            *slot = vector.axis(kind).0;
        }
        values
    }

    #[test]
    fn metric_vectors_are_bitwise_deterministic() {
        // Catalog `req:qmc:determinism`: identical solve inputs yield
        // bitwise-identical vectors within one implementation version — the
        // metrics are a pure function of the resolved output and the inputs.
        let input = ten_measure();
        let a = Engraver::default().solve(&input, &SolverConfig::default());
        let b = Engraver::default().solve(&input, &SolverConfig::default());
        assert_eq!(a.layout.canonical_bytes(), b.layout.canonical_bytes());
        for (x, y) in axes(&a.metric_vector).iter().zip(axes(&b.metric_vector)) {
            assert_eq!(
                x.to_bits(),
                y.to_bits(),
                "metric f64s must be bit-identical"
            );
        }
    }

    #[test]
    fn the_wrapping_fixture_is_measured_honestly() {
        // The ten-measure fixture under the default geometry, measured for
        // real (values pinned loosely; the goldens pin the geometry itself):
        // no cross-column collisions; regular spacing; a single page (the
        // page-fill axis degenerates to exactly 0.0); and — the honest part —
        // greedy first-fit leaves a two-measure stub last system (glyph spans
        // ~78.6 vs ~18.8 staff spaces), which the casting-off axis measures at
        // its clamped worst (CV 0.61 >= the 0.5 anchor -> 1.0). That is the
        // exact "stub final system" failure the catalog says the axis exists
        // to catch; the value is truthful, not a defect in the census.
        let report = Engraver::default().solve(&ten_measure(), &SolverConfig::default());
        let vector = &report.metric_vector;
        assert_eq!(vector.collision_penalty.0, 0.0);
        assert!(vector.spacing_distortion.0 > 0.0 && vector.spacing_distortion.0 < 0.3);
        assert_eq!(vector.slur_shape_penalty.0, 0.0, "vacuous: no drawn slurs");
        assert_eq!(vector.beam_slope_penalty.0, 0.0, "vacuous: no drawn beams");
        assert_eq!(vector.page_fill_efficiency.0, 0.0, "vacuous: single page");
        assert!(
            vector.system_break_penalty.0 > 0.0 && vector.system_break_penalty.0 < 0.35,
            "the non-final system is nearly full: {}",
            vector.system_break_penalty.0
        );
        assert_eq!(
            vector.casting_off_quality.0, 1.0,
            "the stub last line is honestly at the clamped worst"
        );
        assert!(
            vector.symbol_density_uniformity.0 < 0.1,
            "density is even though widths are not: {}",
            vector.symbol_density_uniformity.0
        );
        // The casting-off axis exceeds 0.8 x its threshold in every ratified
        // column, so the SHOULD-level floor diagnostic fires — and, per the
        // catalog, the status is untouched by it.
        assert!(report.warnings.iter().any(|w| matches!(
            w.kind,
            SolverWarningKind::QualityFloorApproached {
                metric: QualityMetricKind::CastingOff
            }
        )));
        assert_eq!(report.status, SolveStatus::Solved);
    }

    #[test]
    fn floor_warnings_reference_the_profiles_threshold_column() {
        // The b-flat scale's spacing distortion (~0.41: eight columns whose
        // flat-bearing columns advance wider) sits between the Standard
        // column's floor (0.8 x 0.40 = 0.32) and the Minimal column's
        // (0.8 x 0.90 = 0.72) — so the default Standard profile warns about
        // Spacing and the Draft profile (which selects the Minimal column per
        // the catalog's profile registry) does not.
        let score = epiphany_testkit::corpus::corpus()
            .into_iter()
            .find(|fixture| fixture.name == "b_flat_major_scale")
            .expect("corpus entry exists");
        let input = to_constrained(&to_logical(&(score.build)()));
        let spacing_warned = |profile: SolverProfile| {
            let config = SolverConfig {
                profile,
                ..SolverConfig::default()
            };
            Engraver::default()
                .solve(&input, &config)
                .warnings
                .iter()
                .any(|w| {
                    matches!(
                        w.kind,
                        SolverWarningKind::QualityFloorApproached {
                            metric: QualityMetricKind::Spacing
                        }
                    )
                })
        };
        assert!(spacing_warned(SolverProfile::Standard));
        assert!(spacing_warned(SolverProfile::Publication));
        assert!(!spacing_warned(SolverProfile::Draft));
        // The metric itself is profile-independent — only the diagnostic
        // column changes.
        let value = Engraver::default()
            .solve(&input, &SolverConfig::default())
            .metric_vector
            .spacing_distortion
            .0;
        assert!((0.32..=0.72).contains(&value), "spacing = {value}");
    }

    #[test]
    fn a_malformed_input_stays_unmeasured() {
        // A structurally invalid input has no trustworthy geometry: the vector
        // is the honest all-worst placeholder, not a vacuous all-best zero.
        let mut input = ten_measure();
        input.glyphs[0].baseline = epiphany_layout_ir::Point::new(f32::NAN, 0.0);
        let report = Engraver::default().solve(&input, &SolverConfig::default());
        assert_eq!(report.status, SolveStatus::InternalError);
        assert_eq!(report.metric_vector, QualityMetricVector::unmeasured());
        // ... and no floor diagnostics are derived from a placeholder.
        assert!(!report
            .warnings
            .iter()
            .any(|w| matches!(w.kind, SolverWarningKind::QualityFloorApproached { .. })));
    }

    #[test]
    fn realized_inter_system_gaps_measure_the_band_models_preferred_height() {
        // The casting pass stacks systems at the vertical-band constructor's
        // preferred inter-system gap, so the vertical-density axis measures
        // realized == preferred (raw 0.0) on the wrapping fixture — the honest
        // near-zero the catalog's rationale describes, *measured* from the
        // resolved page tree rather than assumed.
        let report = Engraver::default().solve(&ten_measure(), &SolverConfig::default());
        assert_eq!(report.metric_vector.vertical_density_penalty.0, 0.0);
    }
}
