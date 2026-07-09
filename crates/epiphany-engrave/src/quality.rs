//! **Real quality-metric computation** — the nine normative axes of the
//! *Quality Metric Catalog* companion (v0.2.0, Chapter 3), measured over what
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
//! * **`spacing_distortion`** — per-system advances between **rhythmic**
//!   columns: the distinct resolved x of each note/rest-bearing slot realized
//!   in the system (its first member's baseline — the spacing pass's own column
//!   reference). The clef/key/time lead and barlines bear no notehead or rest,
//!   so they contribute no column and a note-to-note advance spans them
//!   (catalog §`spacing_distortion` — measuring rhythmic spacing, not furniture).
//! * **`slur_shape_penalty`** — **measured** (Push 3): each drawn slur's arc
//!   ratio `ρ = apex height / chord length` is penalized by its distance
//!   outside the shallow-arc band `[0.08, 0.25]` (catalog §`slur_shape`),
//!   measured over the *spaced* whole curves (the drawn shape, one unit per
//!   slur — not the cast's per-system fragments). The Minimal tier's mid-span
//!   slurs sit at `ρ ≈ 0.16` (in band, 0), but its fixed height clamps push
//!   short slurs above the band (too bulgy) and very long ones below it (too
//!   flat) — a real non-zero value. A curve-free layout measures 0 by the
//!   vacuous-geometry rule. **`beam_slope_penalty`** stays
//!   **vacuous 0.0**: no beam geometry is drawn yet (beams exist logically, not
//!   as segments), so its contributing-unit set is empty.
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
    inter_staff_gap_id, ConstrainedLayoutIR, Curve, QualityMetricVector, SolverWarning,
    SolverWarningKind, SpringSlotId, VerticalBand, VerticalBandId, VerticalBandKind,
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

/// How many points a slur curve is sampled at to find its apex height. Even, so
/// a sample lands on the symmetric arc's `t = 0.5` apex exactly.
const SLUR_APEX_SAMPLES: usize = 32;

/// The raw `slur_shape_penalty` measurement (Quality Metric Catalog
/// §`slur_shape_penalty`): for each drawn slur curve with chord length `c > 0`
/// — the chord being the segment between the curve's endpoints and the apex
/// height `h` the maximum perpendicular distance from the curve to that chord —
/// the arc ratio `ρ = h / c` is penalized by its distance outside the shallow-
/// arc band `[0.08, 0.25]` (`max(0, 0.08 − ρ, ρ − 0.25)`); the axis raw value
/// is the arithmetic mean over units. A curve-free layout has no units, so the
/// mean is `0` (the vacuous-geometry rule) — not by construction but by
/// measurement.
///
/// Measured over the **whole spaced** slur curves — post-horizontal-remap (the
/// drawn shape) and pre-cast-split (one unit per slur) — *not* the cast
/// output's per-system fragments: casting splits a break-spanning slur into
/// sub-cubics whose diagonal chords each read flatter than the whole arc, which
/// would spuriously penalize (and double-count) a slur that is ideally shaped
/// as a whole. The catalog's property "a tier that draws the ideal shallow arc
/// for every slur measures 0" holds only when the whole arc is the unit.
/// Measuring the *spaced* (not constrained) curves means horizontal re-spacing
/// that flattens or steepens a drawn slur is honestly captured here rather than
/// hidden — the units are "drawn slurs" (catalog §`slur_shape`).
fn slur_shape_raw(spaced_curves: &[Curve]) -> f64 {
    let per_curve: Vec<f64> = spaced_curves
        .iter()
        .filter_map(|curve| {
            let cp = curve.control_points();
            let (a, b) = (point(cp[0]), point(cp[3]));
            let chord = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
            if chord <= 0.0 {
                return None; // c > 0 required
            }
            let apex = (0..=SLUR_APEX_SAMPLES)
                .map(|i| {
                    let t = i as f32 / SLUR_APEX_SAMPLES as f32;
                    perp_distance(a, b, cubic_point(cp, t))
                })
                .fold(0.0_f64, f64::max);
            let rho = apex / chord;
            Some((0.08 - rho).max(rho - 0.25).max(0.0))
        })
        .collect();
    mean_or_zero(&per_curve)
}

/// A layout point as exact `f64`.
fn point(p: epiphany_layout_ir::Point) -> (f64, f64) {
    (f64::from(p.x.0), f64::from(p.y.0))
}

/// The cubic-Bézier point at parameter `t`, as `f64`.
fn cubic_point(cp: [epiphany_layout_ir::Point; 4], t: f32) -> (f64, f64) {
    let (u, t) = (f64::from(1.0 - t), f64::from(t));
    let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
    (
        w[0] * f64::from(cp[0].x.0)
            + w[1] * f64::from(cp[1].x.0)
            + w[2] * f64::from(cp[2].x.0)
            + w[3] * f64::from(cp[3].x.0),
        w[0] * f64::from(cp[0].y.0)
            + w[1] * f64::from(cp[1].y.0)
            + w[2] * f64::from(cp[2].y.0)
            + w[3] * f64::from(cp[3].y.0),
    )
}

/// Perpendicular distance from point `p` to the line through `a` and `b`
/// (0 when `a == b`).
fn perp_distance(a: (f64, f64), b: (f64, f64), p: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len <= 0.0 {
        return 0.0;
    }
    ((p.0 - a.0) * dy - (p.1 - a.1) * dx).abs() / len
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
    /// Reference x of each **rhythmic** column (a slot bearing a notehead or
    /// rest) per system, ascending and distinct — the spacing axis's domain
    /// (catalog §`spacing_distortion`: the clef/key/time lead and barlines are
    /// furniture, not rhythmic columns).
    columns: Vec<Vec<f64>>,
}

/// Whether a glyph is a notehead or a rest — the mark that makes its slot a
/// **rhythmic column** (catalog §`spacing_distortion`).
fn is_rhythmic(name: &str) -> bool {
    name.starts_with("notehead") || name.starts_with("rest")
}

fn census(input: &ConstrainedLayoutIR, cast: &CastLayout) -> SystemCensus {
    let count = cast.region_of_system.len();
    let mut members: Vec<Vec<usize>> = vec![Vec::new(); count];
    let mut spans: Vec<Option<(f64, f64)>> = vec![None; count];
    // Rhythmic slots: those bearing a notehead or rest. Precomputed because a
    // slot's rhythmic status depends on *all* its members (a note's accidental
    // may precede its notehead in input order), and only rhythmic slots become
    // spacing columns (catalog §`spacing_distortion`, "rhythmic column").
    let rhythmic: BTreeSet<SpringSlotId> = input
        .glyphs
        .iter()
        .filter(|glyph| is_rhythmic(glyph.glyph.as_str()))
        .map(|glyph| glyph.horizontal_slot)
        .collect();
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
        // Only rhythmic columns carry spacing advances; a note-to-note advance
        // spans any intervening barline or furniture (which contribute no column).
        if rhythmic.contains(&glyph.horizontal_slot) {
            columns[system]
                .entry(glyph.horizontal_slot)
                .or_insert_with(|| f64::from(cast.glyphs[index].position.x.0));
        }
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
/// **rhythmic** column advances, over systems realizing at least three rhythmic
/// columns (note/rest-bearing slots — see [`census`]).
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
    // The realized gap between two staves is measured over their FULL content —
    // every glyph, stroke, and curve the band owns — not over the band's glyph
    // `members` alone. A staff's ink is mostly not glyphs: ledger lines, stems,
    // and slurs routinely reach further from the staff than any notehead, and
    // the inter-staff solve separates staves until their *content* clears the
    // declared gap. Measuring glyphs only scored a correctly separated system
    // against a gap it never targeted, reporting the ledger and slur ink the
    // solve had already accounted for as crowding.
    //
    // Ownership comes from each primitive's declared `vertical_band`
    // (`req:layoutir:primitive-band-ownership`); the geometry is read back from
    // the BAKED output, so a shift the bake failed to apply to some primitive
    // class surfaces here as a real deviation rather than hiding behind the
    // solver's own intent.
    let system_of_glyph = |index: usize| -> Option<usize> {
        cast.system_of_slot
            .get(&input.glyphs[index].horizontal_slot)
            .copied()
    };
    let mut content: BTreeMap<(usize, VerticalBandId), (f64, f64)> = BTreeMap::new();
    {
        let mut add = |system: usize, band: VerticalBandId, lo: f64, hi: f64| {
            let entry = content
                .entry((system, band))
                .or_insert((f64::INFINITY, f64::NEG_INFINITY));
            entry.0 = entry.0.min(lo);
            entry.1 = entry.1.max(hi);
        };
        for (index, glyph) in input.glyphs.iter().enumerate() {
            if let Some(system) = system_of_glyph(index) {
                let ink = ink_box(cast, input, index);
                add(system, glyph.vertical_band, ink[1], ink[3]);
            }
        }
        for (index, stroke) in cast.strokes.iter().enumerate() {
            let Some(system) = cast.stroke_system[index] else {
                continue;
            };
            let half = f64::from(stroke.thickness.0.max(0.0)) * 0.5;
            let lo = f64::from(stroke.from.y.0.min(stroke.to.y.0)) - half;
            let hi = f64::from(stroke.from.y.0.max(stroke.to.y.0)) + half;
            add(system, stroke.vertical_band, lo, hi);
        }
        for (index, curve) in cast.curves.iter().enumerate() {
            let Some(system) = cast.curve_system[index] else {
                continue;
            };
            let half = f64::from(curve.thickness.0.max(0.0)) * 0.5;
            for point in curve.control_points() {
                let y = f64::from(point.y.0);
                add(system, curve.vertical_band, y - half, y + half);
            }
        }
    }

    for (region_index, region) in input.regions.iter().enumerate() {
        // The region's systems, in page order. A staff band is `to_constrained`'s
        // per-*manifestation* band — one per (staff, region) — so its content can
        // only land in its own region's systems, and "has content in one of this
        // region's systems" identifies the region's staff bands exactly.
        //
        // Membership is NOT read from `VerticalBand::members`. That list holds
        // glyphs, and a staff band is allowed to have none: a staff whose clef is
        // unbundled engraves to an anchor *stroke*, and its staff lines are
        // strokes regardless. Filtering by glyph members would silently drop such
        // a staff from the axis — the same glyph-members assumption this metric
        // exists to shed.
        let region_systems: Vec<usize> = census
            .region
            .iter()
            .enumerate()
            .filter(|&(_, &r)| r == region_index)
            .map(|(system, _)| system)
            .collect();
        let Some(&first_system) = region_systems.first() else {
            continue;
        };
        // Top staff first, ordered within the region's first system: a system
        // translates rigidly as a whole, so the staff ORDER is the same in each
        // (only the gaps between them are renegotiated per system).
        let mut staves: Vec<(f64, VerticalBandId)> = input
            .vertical_bands
            .iter()
            .filter(|band| matches!(band.kind, VerticalBandKind::Staff(_)))
            .filter_map(|band| {
                content
                    .get(&(first_system, band.id))
                    .map(|&(_, top)| (top, band.id))
            })
            .collect();
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
            let (upper, lower) = (staves[gap - 1].1, staves[gap].1);
            // EVERY system realizing the pair contributes a unit, not just the
            // first. The inter-staff solve sizes each system's gaps from that
            // system's own content, so one band's realized height genuinely
            // differs across the systems of a region — a later system may carry
            // different pressure, or a bake bug touching one primitive class.
            // Measuring only the first would average away both.
            for &system in &region_systems {
                let (Some(&(upper_bottom, _)), Some(&(_, lower_top))) =
                    (content.get(&(system, upper)), content.get(&(system, lower)))
                else {
                    continue;
                };
                let realized = (upper_bottom - lower_top).max(0.0);
                per_unit.push((realized - preferred).abs() / preferred);
            }
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
    spaced_curves: &[Curve],
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
        // Slurs draw (E2), so their shape is now MEASURED (Push 3), not pinned:
        // each drawn slur's arc ratio ρ = apex height / chord length is
        // penalized by its distance outside the shallow-arc band [0.08, 0.25].
        // Measured on the SPACED whole curves — post-horizontal-remap (the
        // shape the reader sees), pre-cast-split (the whole slur, one unit) —
        // so re-spacing that visibly flattens or steepens a slur is caught, yet
        // a well-shaped slur that merely crosses a system break is not
        // penalized by its fragments' diagonal chords. A curve-free layout
        // measures 0 by the vacuous-geometry rule.
        slur_shape_penalty: normalize(slur_shape_raw(spaced_curves), anchors::SLUR_SHAPE_R_WORST),
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
        // page-fill axis degenerates to exactly 0.0). Greedy first-fit alone
        // would leave a two-measure stub last system (width CV 0.61 -> the
        // clamped worst 1.0); casting-off's widow-rebalance evens that into a
        // six/four split (system widths ~59.5 vs ~37.8 staff spaces, CV ~0.22),
        // so casting_off measures ~0.45 — comfortably inside the Minimal 0.90
        // threshold. The trade-off is on the break axis: the non-final system
        // is deliberately left ~66% full (not greedy's ~87%), so system_break
        // rises to ~0.68 — still well inside Minimal. Both axes sit above the
        // Standard profile's floor (0.8 x 0.35 = 0.28), so both fire the
        // SHOULD-level diagnostic, which per the catalog never changes status.
        let report = Engraver::default().solve(&ten_measure(), &SolverConfig::default());
        let vector = &report.metric_vector;
        assert_eq!(vector.collision_penalty.0, 0.0);
        assert!(vector.spacing_distortion.0 > 0.0 && vector.spacing_distortion.0 < 0.3);
        // The ten-measure fixture has no drawn slur curve, so the axis measures
        // 0.0 by the vacuous-geometry rule (an empty contributing-unit set).
        assert_eq!(vector.slur_shape_penalty.0, 0.0, "vacuous: no drawn slurs");
        assert_eq!(vector.beam_slope_penalty.0, 0.0, "vacuous: no drawn beams");
        assert_eq!(vector.page_fill_efficiency.0, 0.0, "vacuous: single page");
        // Per-system justification fills every NON-final system to the content
        // width, so the system-break axis (which measures a non-final system's
        // shortfall) collapses to near zero — the point of justification.
        assert!(
            vector.system_break_penalty.0 < 0.1,
            "the non-final system fills the width after justification: {}",
            vector.system_break_penalty.0
        );
        // The width-uniformity axes are non-zero: the justified non-final system
        // spans the full width while the last stays ragged-right, so the two
        // systems' ink widths (and their glyph densities) differ. Optimal
        // casting-off balances the split (5/4 measures, not greedy's 6-plus/stub),
        // which fills the final system more and pulls casting_off DOWN from the
        // greedy value (~0.80 → ~0.61) — the break search's payoff. Still honest
        // for a two-system score (one full line, one shorter last line); a metric
        // refinement that scores casting-off on natural widths or excludes the
        // intentionally-ragged final system is a follow-up (see DECISIONS).
        assert!(
            (0.5..0.7).contains(&vector.casting_off_quality.0),
            "the balanced split leaves a moderate full-vs-ragged contrast: {}",
            vector.casting_off_quality.0
        );
        assert!(
            (0.45..0.65).contains(&vector.symbol_density_uniformity.0),
            "density differs between the stretched and the ragged system: {}",
            vector.symbol_density_uniformity.0
        );
        // The casting-off axis exceeds 0.8 × its Standard threshold, so its
        // SHOULD-level floor diagnostic fires (per the catalog, never changing
        // status); the system-break axis, now near zero, no longer floors.
        let floored = |metric: QualityMetricKind| {
            report.warnings.iter().any(|w| {
                matches!(w.kind, SolverWarningKind::QualityFloorApproached { metric: m } if m == metric)
            })
        };
        assert!(floored(QualityMetricKind::CastingOff));
        assert!(!floored(QualityMetricKind::SystemBreak));
        assert_eq!(report.status, SolveStatus::Solved);
    }

    #[test]
    fn slur_shape_is_measured_penalizing_out_of_band_arcs() {
        use epiphany_core::{Slur, SlurId, SlurKind, SpanStyle};
        use epiphany_layout_ir::to_constrained;

        let with_slur = |start: usize, end: usize| {
            let mut s = epiphany_testkit::fixtures::ten_measure_single_staff(0x000A_11CE);
            let ev: Vec<_> = s.canvas.regions[0].staff_instances()[0].voices[0]
                .events
                .clone();
            s.cross_cutting.slurs.push(Slur {
                id: s.identity.mint::<SlurId>(),
                start_event: ev[start],
                end_event: ev[end],
                kind: SlurKind::Legato,
                curvature_override: None,
                style: SpanStyle::default(),
            });
            Engraver::default()
                .solve(&to_constrained(&to_logical(&s)), &SolverConfig::default())
                .metric_vector
                .slur_shape_penalty
                .0
        };
        // A slur over adjacent events: the min-height clamp forces a tall arc
        // over a tiny chord (ρ well above the 0.25 band), a real bulge penalty.
        assert!(
            with_slur(0, 1) > 0.0,
            "a bulgy short slur is penalized: {}",
            with_slur(0, 1)
        );
        // A slur over a wide span: the auto height gives ρ ≈ 0.16, inside the
        // ideal band [0.08, 0.25], so no penalty.
        assert_eq!(
            with_slur(0, 6),
            0.0,
            "an in-band (mid-span) slur is not penalized"
        );
    }

    #[test]
    fn a_break_spanning_in_band_slur_measures_zero_despite_splitting() {
        // A well-shaped (ρ ≈ 0.16, in-band) slur whose span crosses a system
        // break splits into per-system sub-curves. The shape axis measures the
        // WHOLE slur (the constrained curve), not the fragments — whose diagonal
        // chords would each read too flat — so the well-shaped slur still scores
        // 0, as the catalog's "ideal arc ⇒ 0" property requires.
        use epiphany_core::{Slur, SlurId, SlurKind, SpanStyle, TypedObjectId};
        use epiphany_layout_ir::to_constrained;

        let mut score = epiphany_testkit::fixtures::ten_measure_single_staff(0x000A_11CE);
        let ev: Vec<_> = score.canvas.regions[0].staff_instances()[0].voices[0]
            .events
            .clone();
        let id: SlurId = score.identity.mint();
        // Events 14→24 straddle the fixture's centred two-system break (optimal
        // casting-off balances to ~5/4 measures), wide enough that the height
        // clamp does not bind → ρ ≈ 0.16.
        score.cross_cutting.slurs.push(Slur {
            id,
            start_event: ev[14],
            end_event: ev[24],
            kind: SlurKind::Legato,
            curvature_override: None,
            style: SpanStyle::default(),
        });
        let report = Engraver::default().solve(
            &to_constrained(&to_logical(&score)),
            &SolverConfig::default(),
        );
        // The slur really did split (the fragment path would have fired)…
        let segments = report
            .layout
            .curves
            .iter()
            .filter(|c| c.provenance.source == TypedObjectId::Slur(id))
            .count();
        assert!(segments >= 2, "the slur splits, got {segments} segment(s)");
        // …yet the shape axis is 0: the whole arc is in-band.
        assert_eq!(
            report.metric_vector.slur_shape_penalty.0, 0.0,
            "a split but well-shaped slur is not spuriously penalized"
        );
    }

    #[test]
    fn floor_warnings_reference_the_profiles_threshold_column() {
        // The floor warning references the PROFILE'S threshold column, not a
        // fixed threshold: the Standard column's casting-off threshold is
        // tighter than the Minimal column's, so a value BETWEEN their floors
        // warns under Standard/Publication (the Standard column) but not under
        // Draft (the Minimal column). Tested directly on `floor_warnings` with a
        // synthetic value between the two floors — robust to how any particular
        // fixture's casting-off happens to land (justification, for one, now
        // drives the ten-measure fixture's casting-off above both floors).
        use epiphany_layout_ir::quality::{
            profile_thresholds, MINIMAL_THRESHOLDS, QUALITY_FLOOR_FRACTION, STANDARD_THRESHOLDS,
        };
        use epiphany_layout_ir::NormalizedMetric;

        let standard_floor = QUALITY_FLOOR_FRACTION * STANDARD_THRESHOLDS.casting_off_quality;
        let minimal_floor = QUALITY_FLOOR_FRACTION * MINIMAL_THRESHOLDS.casting_off_quality;
        assert!(
            standard_floor < minimal_floor,
            "the Standard casting-off column is tighter than the Minimal one"
        );
        let between = 0.5 * (standard_floor + minimal_floor);
        // Only casting_off matters here; the other axes' values are irrelevant
        // (the closure inspects the CastingOff warning alone).
        let vector = QualityMetricVector {
            casting_off_quality: NormalizedMetric(between),
            ..QualityMetricVector::unmeasured()
        };
        let castoff_warned = |profile: SolverProfile| {
            super::floor_warnings(&vector, profile_thresholds(profile))
                .iter()
                .any(|w| {
                    matches!(
                        w.kind,
                        SolverWarningKind::QualityFloorApproached {
                            metric: QualityMetricKind::CastingOff
                        }
                    )
                })
        };
        assert!(castoff_warned(SolverProfile::Standard));
        assert!(castoff_warned(SolverProfile::Publication));
        assert!(!castoff_warned(SolverProfile::Draft));
    }

    #[test]
    fn short_scores_do_not_trip_the_standard_spacing_floor() {
        // P12-I12 regression. Before `spacing_distortion` was scoped to rhythmic
        // columns, a short healthy line's wide clef-to-first-note lead advance
        // inflated the CV above the Standard warning floor (0.8 x 0.40 = 0.32),
        // so these three-to-eight-column corpus entries spuriously warned
        // (measured 0.36-0.41). Scoped to note/rest columns — the clef/key/time
        // lead bears no notehead, so it contributes no column — none does.
        let floor = 0.8 * 0.40;
        for name in ["b_flat_major_scale", "notes_and_rests", "meter_three_four"] {
            let score = epiphany_testkit::corpus::corpus()
                .into_iter()
                .find(|fixture| fixture.name == name)
                .expect("corpus entry exists");
            let input = to_constrained(&to_logical(&(score.build)()));
            let report = Engraver::default().solve(&input, &SolverConfig::default());
            assert!(
                report.metric_vector.spacing_distortion.0 < floor,
                "{name}: spacing {} still above the Standard floor {floor}",
                report.metric_vector.spacing_distortion.0
            );
            assert!(
                !report.warnings.iter().any(|w| matches!(
                    w.kind,
                    SolverWarningKind::QualityFloorApproached {
                        metric: QualityMetricKind::Spacing
                    }
                )),
                "{name}: spurious spacing floor warning under the default Standard profile"
            );
        }
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

    #[test]
    fn vertical_justification_trades_page_fill_for_inter_system_density() {
        use crate::PageGeometry;
        use epiphany_layout_ir::{Margins, Size2D, StaffSpace};
        // On a MULTI-page layout, vertical justification spreads a non-final
        // page's systems to fill the height (`page_fill` → ~0), which stretches
        // its inter-system gaps beyond the band model's preferred height — a
        // cost the vertical-density axis measures. The two axes trade: filling
        // the page is paid for in inter-system density. A catalog refinement
        // that scores only EXCESS stretch (or measures gap uniformity) is a
        // deferred follow-up, alongside the casting_off / justified-raggedness
        // note. Pinned so the interaction is not silent (review finding).
        let geometry = PageGeometry {
            size: Size2D {
                width: StaffSpace(40.0),
                height: StaffSpace(30.0),
            },
            margins: Margins {
                top: StaffSpace(5.0),
                right: StaffSpace(5.0),
                bottom: StaffSpace(5.0),
                left: StaffSpace(5.0),
            },
        };
        let report =
            Engraver::with_geometry(geometry).solve(&ten_measure(), &SolverConfig::default());
        assert!(
            report.layout.pages.len() >= 2,
            "the small page forces multiple pages"
        );
        // Justification fills the non-final pages…
        assert!(
            report.metric_vector.page_fill_efficiency.0 < 0.1,
            "non-final pages fill: {}",
            report.metric_vector.page_fill_efficiency.0
        );
        // …at the measured cost of stretched inter-system gaps.
        assert!(
            report.metric_vector.vertical_density_penalty.0 > 0.0,
            "stretched inter-system gaps are measured: {}",
            report.metric_vector.vertical_density_penalty.0
        );
    }
}
