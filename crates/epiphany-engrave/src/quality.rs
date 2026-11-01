//! **Real quality-metric computation** — the nine normative axes of the
//! *Quality Metric Catalog* companion (v0.4.0, Chapter 3), measured over what
//! the pipeline already produced: the resolved world-frame geometry
//! ([`CastLayout`]), the constrained input (slot identity, vertical bands), and
//! the declared page geometry. This module implements the v0.4.0 unit set:
//! `spacing_distortion` over rhythmic columns (v0.2.0) and
//! `vertical_density_penalty` over each *realization* of a gap band (v0.3.0),
//! its raw measurement a gap's line distance against what its floor and its
//! ink ask (v0.4.0). None of the revisions changed what an axis is normalized
//! against: the anchors, threshold tables, and warning fraction are the
//! catalog's, transcribed unchanged in [`epiphany_layout_ir::quality`].
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
//! * **`vertical_density_penalty`** (0.4.0) — each realized gap's line
//!   distance `d`, read from the baked staff lines, against what it asks,
//!   `max(f, s + c, b)`: its band's floor (an `InterStaffGap` band of the
//!   constrained input; for a gap between systems, which `to_constrained`
//!   declares no band for, [`VerticalBand::inter_system_gap`]'s), the skyline
//!   separation `s` of the inks above and below it as baked, column by column
//!   (every staff above an inter-staff gap; the whole systems either side of
//!   an inter-system one), the clearance `c`, and a beam across the two
//!   staves' bound `b`. Crowding and sprawl both deviate.
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

use epiphany_core::TypedObjectId;
use std::collections::{BTreeMap, BTreeSet};

use epiphany_layout_ir::quality::{
    anchors, normalize, MetricThresholds, QUALITY_FLOOR_FRACTION, QUALITY_METRIC_KINDS,
};
use epiphany_layout_ir::{
    inter_staff_gap_id, ConstrainedLayoutIR, Curve, GlyphObjectId, QualityMetricVector,
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
        // The units are slurs; a tie's arc is not one.
        .filter(|curve| matches!(curve.provenance.source, TypedObjectId::Slur(_)))
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
        // W1 pin 7: the published attribution, not a second derivation from
        // `system_of_slot` — one glyph→system rule, computed once in casting.
        let Some(system) = cast.glyph_system[index] else {
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

/// `vertical_density_penalty` (catalog §`vertical_density_penalty`, 0.4.0):
/// mean relative deviation `|d − d*| / d*` of each realized gap's line
/// distance from what its floor and its ink ask (see the module docs).
fn vertical_raw(input: &ConstrainedLayoutIR, cast: &CastLayout, census: &SystemCensus) -> f64 {
    let units = vertical_units(input, cast, census);
    mean_or_zero(&[units.inter_staff, units.inter_system].concat())
}

/// The axis's contributing units, split by kind (catalog §`vertical_density_penalty`:
/// "each *realization* … contributes one unit per system").
///
/// Split out so tests can assert the unit **set**, not merely its mean.
struct VerticalUnits {
    inter_staff: Vec<f64>,
    inter_system: Vec<f64>,
}

/// The catalog's clearances (`req:qmc:vertical`): `c` between inks up and
/// down, `h` across within which they meet, and how much further a lyric
/// reaches on the side it stands. Fixed here, in the measure, so that it
/// does not depend on the solver it judges.
const VERTICAL_CLEARANCE: f64 = 0.5;
const HORIZONTAL_CLEARANCE: f64 = 0.25;
const LYRIC_REACH: f64 = 1.0;

/// One primitive's ink box, `[left, bottom, right, top]`, with how much
/// further it reaches below and above for the measure (a lyric's padding).
type Ink = ([f64; 4], f64, f64);

/// An `f64` ordered totally, for the sweep's heap.
#[derive(Copy, Clone, PartialEq)]
struct Total(f64);

impl Eq for Total {}

impl PartialOrd for Total {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Total {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

/// A step function across x: on `[edges[i], edges[i + 1])` the value
/// `values[i]`, the fill (infinite) where nothing covers it.
#[derive(Clone, Default)]
struct Steps {
    edges: Vec<f64>,
    values: Vec<f64>,
}

impl Steps {
    /// The least (`lowest`) or greatest value of the spans `(left, right,
    /// value)` covering each interval between their edges.
    fn of(spans: &[(f64, f64, f64)], lowest: bool) -> Steps {
        let sign = if lowest { -1.0 } else { 1.0 };
        let mut sorted = spans.to_vec();
        sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut edges: Vec<f64> = sorted.iter().flat_map(|&(l, r, _)| [l, r]).collect();
        edges.sort_by(f64::total_cmp);
        edges.dedup();
        let mut heap: std::collections::BinaryHeap<(Total, Total)> =
            std::collections::BinaryHeap::new();
        let (mut values, mut next) = (Vec::new(), 0);
        for w in edges.windows(2) {
            while next < sorted.len() && sorted[next].0 <= w[0] {
                heap.push((Total(sign * sorted[next].2), Total(sorted[next].1)));
                next += 1;
            }
            while heap.peek().is_some_and(|(_, end)| end.0 <= w[0]) {
                heap.pop();
            }
            values.push(
                heap.peek()
                    .map_or(sign * f64::NEG_INFINITY, |(v, _)| sign * v.0),
            );
        }
        Steps { edges, values }
    }

    fn fill(lowest: bool) -> f64 {
        if lowest {
            f64::INFINITY
        } else {
            f64::NEG_INFINITY
        }
    }

    /// The value on the interval starting at or before `x`.
    fn at(&self, x: f64, lowest: bool) -> f64 {
        let i = self.edges.partition_point(|&e| e <= x);
        if i == 0 || i >= self.edges.len() {
            return Steps::fill(lowest);
        }
        self.values[i - 1]
    }

    /// The pointwise least (`lowest`) or greatest of two step functions.
    fn merge(&self, other: &Steps, lowest: bool) -> Steps {
        let mut edges: Vec<f64> = self.edges.iter().chain(&other.edges).copied().collect();
        edges.sort_by(f64::total_cmp);
        edges.dedup();
        let pick = |a: f64, b: f64| if lowest { a.min(b) } else { a.max(b) };
        let values = edges
            .windows(2)
            .map(|w| pick(self.at(w[0], lowest), other.at(w[0], lowest)))
            .collect();
        Steps { edges, values }
    }
}

/// Whether an ink box has positive extent both ways, finite.
fn held(b: &[f64; 4]) -> bool {
    b.iter().all(|v| v.is_finite()) && b[2] > b[0] && b[3] > b[1]
}

/// The least bottom, less its reach down, of each interval across, every box
/// widened by `h` on either side: the ink above a gap, as it faces the gap.
fn lowest_bottoms(ink: &[Ink]) -> Steps {
    let spans: Vec<(f64, f64, f64)> = ink
        .iter()
        .filter(|(b, ..)| held(b))
        .map(|(b, down, _)| {
            (
                b[0] - HORIZONTAL_CLEARANCE,
                b[2] + HORIZONTAL_CLEARANCE,
                b[1] - down,
            )
        })
        .collect();
    Steps::of(&spans, true)
}

/// The greatest top, with its reach up, of each interval across: the ink
/// below a gap, as it faces the gap.
fn highest_tops(ink: &[Ink]) -> Steps {
    let spans: Vec<(f64, f64, f64)> = ink
        .iter()
        .filter(|(b, ..)| held(b))
        .map(|(b, _, up)| (b[0], b[2], b[3] + up))
        .collect();
    Steps::of(&spans, false)
}

/// The skyline separation `s` of `req:qmc:vertical`: over every pair of an
/// ink above the gap and one below it that meet across (their extents of
/// positive size, the upper's widened by `h` on each side, overlapping), the
/// greatest depth the upper reaches below `line_above` plus the height the
/// lower reaches above `line_below`; minus infinity where no pair meets. Two
/// boxes meet exactly where some interval between all the boxes' edges lies
/// under both, so the greatest over those intervals is the greatest over
/// the pairs.
fn separation(above: &Steps, below: &Steps, line_above: f64, line_below: f64) -> f64 {
    let mut edges: Vec<f64> = above.edges.iter().chain(&below.edges).copied().collect();
    edges.sort_by(f64::total_cmp);
    edges.dedup();
    edges
        .windows(2)
        .filter_map(|w| {
            let (low, high) = (above.at(w[0], true), below.at(w[0], false));
            (low.is_finite() && high.is_finite())
                .then_some((line_above - low) + (high - line_below))
        })
        .fold(f64::NEG_INFINITY, f64::max)
}

/// One unit's raw value: the realized line distance `d` against what the gap
/// asks, `max(f, s + c, b)`.
fn vertical_unit(d: f64, floor: f64, separation: f64, beam: f64) -> Option<f64> {
    let asked = floor.max(separation + VERTICAL_CLEARANCE).max(beam);
    (asked > 0.0 && d.is_finite()).then(|| (d - asked).abs() / asked)
}

fn vertical_units(
    input: &ConstrainedLayoutIR,
    cast: &CastLayout,
    census: &SystemCensus,
) -> VerticalUnits {
    // --- Every primitive's ink, read back from the BAKED output ------------
    // Ownership comes from each primitive's declared `vertical_band`
    // (`req:layoutir:primitive-band-ownership`); the geometry is the resolved
    // output's, so a shift the bake failed to apply to some primitive class
    // surfaces here as a deviation rather than hiding behind the solver's
    // own record. W1 pin 7: the published attribution (`glyph_system` and
    // its siblings), not a second derivation.
    //
    // A beam across two staves stands between them, the content of neither:
    // it enters the measure through its bound `b` alone.
    let between: BTreeSet<GlyphObjectId> = input
        .cross_staff_beams
        .iter()
        .flat_map(|beam| beam.ink.iter().copied())
        .collect();
    // A lyric's ink by id, and whether it stands above its staff.
    let mut lyric: BTreeMap<GlyphObjectId, bool> = BTreeMap::new();
    for mark in &input.family {
        if let Some(p) = mark
            .placement
            .filter(|p| p.order == epiphany_layout_ir::PlaceOrder::Lyric)
        {
            for id in mark.glyphs.iter().chain(&mark.strokes).chain(&mark.texts) {
                lyric.insert(*id, p.above);
            }
        }
    }
    let staff_band: BTreeSet<VerticalBandId> = input
        .vertical_bands
        .iter()
        .filter(|band| matches!(band.kind, VerticalBandKind::Staff(_)))
        .map(|band| band.id)
        .collect();
    let mut ink: BTreeMap<(usize, VerticalBandId), Vec<Ink>> = BTreeMap::new();
    let mut put = |system: usize, band: VerticalBandId, id: Option<GlyphObjectId>, b: [f64; 4]| {
        if id.is_some_and(|id| between.contains(&id)) || !staff_band.contains(&band) {
            return;
        }
        let (down, up) = match id.and_then(|id| lyric.get(&id)) {
            Some(true) => (0.0, LYRIC_REACH),
            Some(false) => (LYRIC_REACH, 0.0),
            None => (0.0, 0.0),
        };
        ink.entry((system, band)).or_default().push((b, down, up));
    };
    let f = f64::from;
    for (index, glyph) in cast.glyphs.iter().enumerate() {
        let Some(system) = cast.glyph_system[index] else {
            continue;
        };
        let (band, id) = match input.glyphs.get(index) {
            Some(source) => (source.vertical_band, Some(source.id())),
            None => (cast.appended_bands[index - input.glyphs.len()], None),
        };
        let (x, y, b) = (
            f(glyph.position.x.0),
            f(glyph.position.y.0),
            glyph.bounding_box,
        );
        put(
            system,
            band,
            id,
            [
                x + f(b.left.0),
                y + f(b.bottom.0),
                x + f(b.right.0),
                y + f(b.top.0),
            ],
        );
    }
    // Each staff's lowest and highest line in each system.
    let mut lines: BTreeMap<(usize, VerticalBandId), (f64, f64)> = BTreeMap::new();
    for (index, stroke) in cast.strokes.iter().enumerate() {
        let Some(system) = cast.stroke_system[index] else {
            continue;
        };
        let half = f(stroke.thickness.0.max(0.0)) * 0.5;
        let (x0, x1) = (f(stroke.from.x.0), f(stroke.to.x.0));
        let (y0, y1) = (f(stroke.from.y.0), f(stroke.to.y.0));
        put(
            system,
            stroke.vertical_band,
            Some(stroke.id()),
            [
                x0.min(x1) - half,
                y0.min(y1) - half,
                x0.max(x1) + half,
                y0.max(y1) + half,
            ],
        );
        if matches!(stroke.provenance.source, TypedObjectId::Staff(_)) && y0 == y1 {
            let entry = lines
                .entry((system, stroke.vertical_band))
                .or_insert((y0, y0));
            entry.0 = entry.0.min(y0);
            entry.1 = entry.1.max(y0);
        }
    }
    // A curve by twelve chords of equal parameter step.
    for (index, curve) in cast.curves.iter().enumerate() {
        let Some(system) = cast.curve_system[index] else {
            continue;
        };
        let half = f(curve.thickness.0.max(0.0)) * 0.5;
        let cp = curve.control_points();
        let at = |t: f64| {
            let u = 1.0 - t;
            let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
            (
                (0..4).map(|k| w[k] * f(cp[k].x.0)).sum::<f64>(),
                (0..4).map(|k| w[k] * f(cp[k].y.0)).sum::<f64>(),
            )
        };
        let mut last = at(0.0);
        for k in 1..=12 {
            let next = at(f64::from(k) / 12.0);
            put(
                system,
                curve.vertical_band,
                Some(curve.id()),
                [
                    last.0.min(next.0) - half,
                    last.1.min(next.1) - half,
                    last.0.max(next.0) + half,
                    last.1.max(next.1) + half,
                ],
            );
            last = next;
        }
    }
    for (system, resolved) in cast.pages.iter().flat_map(|p| &p.systems).enumerate() {
        for &i in &resolved.primitives.texts {
            let (text, source) = (&cast.texts[i as usize], &input.texts[i as usize]);
            let (x, y, b) = (f(text.origin.x.0), f(text.origin.y.0), text.bounds);
            put(
                system,
                source.vertical_band,
                Some(text.id()),
                [
                    x + f(b.left.0),
                    y + f(b.bottom.0),
                    x + f(b.right.0),
                    y + f(b.top.0),
                ],
            );
        }
    }
    // Each system's staves, top first by their highest line.
    let staves_of = |system: usize| -> Vec<(VerticalBandId, (f64, f64))> {
        // A staff of one line counts its lines two spaces either side of it.
        let mut staves: Vec<(VerticalBandId, (f64, f64))> = lines
            .range((system, VerticalBandId(0))..=(system, VerticalBandId(u128::MAX)))
            .map(|(&(_, band), &(lo, hi))| {
                (
                    band,
                    if hi - lo < 1e-3 {
                        (lo - 2.0, hi + 2.0)
                    } else {
                        (lo, hi)
                    },
                )
            })
            .collect();
        staves.sort_by(|a, b| b.1 .1.total_cmp(&a.1 .1).then(a.0.cmp(&b.0)));
        staves
    };
    let none: Vec<Ink> = Vec::new();
    let ink_of = |system: usize, band: VerticalBandId| ink.get(&(system, band)).unwrap_or(&none);

    // --- The bound a beam across two staves keeps --------------------------
    // The least line distance at which every stem reaching it keeps its
    // length: the two staves' line distance in the constrained frame less how
    // far the lower may rise from there (`rise_limit`), in the system the
    // beam's ink landed in.
    let mut constrained_lines: BTreeMap<VerticalBandId, (f64, f64)> = BTreeMap::new();
    for stroke in &input.strokes {
        if matches!(stroke.provenance.source, TypedObjectId::Staff(_))
            && stroke.from.y == stroke.to.y
        {
            let y = f(stroke.from.y.0);
            let entry = constrained_lines
                .entry(stroke.vertical_band)
                .or_insert((y, y));
            entry.0 = entry.0.min(y);
            entry.1 = entry.1.max(y);
        }
    }
    let staff_to_band: BTreeMap<(usize, epiphany_core::StaffId), VerticalBandId> = input
        .vertical_bands
        .iter()
        .filter_map(|band| match band.kind {
            VerticalBandKind::Staff(staff) => Some((staff, band.id)),
            _ => None,
        })
        .filter_map(|(staff, band)| {
            // A staff band belongs to the region whose systems hold its ink.
            let region = (0..census.region.len())
                .find(|&s| lines.contains_key(&(s, band)))
                .map(|s| census.region[s])?;
            Some(((region, staff), band))
        })
        .collect();
    let stroke_system: BTreeMap<GlyphObjectId, usize> = cast
        .strokes
        .iter()
        .zip(&cast.stroke_system)
        .filter_map(|(stroke, system)| Some((stroke.id(), (*system)?)))
        .collect();
    let mut beams: BTreeMap<(usize, VerticalBandId, VerticalBandId), f64> = BTreeMap::new();
    for beam in &input.cross_staff_beams {
        let (Some(&upper), Some(&lower)) = (
            staff_to_band.get(&(beam.region, beam.upper)),
            staff_to_band.get(&(beam.region, beam.lower)),
        ) else {
            continue;
        };
        let (Some(system), Some(&(upper_bottom, _)), Some(&(_, lower_top))) = (
            beam.ink
                .iter()
                .find_map(|id| stroke_system.get(id).copied()),
            constrained_lines.get(&upper),
            constrained_lines.get(&lower),
        ) else {
            continue;
        };
        let least = upper_bottom - lower_top - f64::from(beam.rise_limit);
        let entry = beams
            .entry((system, upper, lower))
            .or_insert(f64::NEG_INFINITY);
        *entry = entry.max(least);
    }

    // --- InterStaffGap units ------------------------------------------------
    // Gap g of a region separates its staves g−1 and g, top first (per
    // `to_constrained`); EVERY system realizing the pair contributes a unit.
    // Above the gap stands every staff above it in the system.
    let mut inter_staff = Vec::new();
    for (region_index, region) in input.regions.iter().enumerate() {
        let region_layout_id = region.provenance.stable_id;
        for (system, _) in census
            .region
            .iter()
            .enumerate()
            .filter(|&(_, &r)| r == region_index)
        {
            let staves = staves_of(system);
            // The ink of every staff above the gap, merged staff by staff.
            let mut above = Steps::default();
            for g in 1..staves.len() {
                let (upper, (line_above, _)) = staves[g - 1];
                let (lower, (_, line_below)) = staves[g];
                above = above.merge(&lowest_bottoms(ink_of(system, upper)), true);
                let gap_id = inter_staff_gap_id(region_layout_id, g);
                let Some(band) = input.vertical_bands.iter().find(|band| band.id == gap_id) else {
                    continue;
                };
                let below = highest_tops(ink_of(system, lower));
                let s = separation(&above, &below, line_above, line_below);
                let b = beams
                    .get(&(system, upper, lower))
                    .copied()
                    .unwrap_or(f64::NEG_INFINITY);
                if let Some(raw) =
                    vertical_unit(line_above - line_below, f(band.min_height.0), s, b)
                {
                    inter_staff.push(raw);
                }
            }
        }
    }

    // --- InterSystemGap units (consecutive systems on a page) ---------------
    // `to_constrained` declares no InterSystemGap band, so the floor is the
    // band constructor's, the one casting reads.
    let floor = f(VerticalBand::inter_system_gap(VerticalBandId(0))
        .min_height
        .0);
    let mut inter_system = Vec::new();
    let mut first = 0;
    for page in &cast.pages {
        for k in 1..page.systems.len() {
            let (upper, lower) = (first + k - 1, first + k);
            let (above, below) = (staves_of(upper), staves_of(lower));
            let (Some(&(_, (line_above, _))), Some(&(_, (_, line_below)))) =
                (above.last(), below.first())
            else {
                continue;
            };
            let gather = |system: usize, staves: &[(VerticalBandId, (f64, f64))]| -> Vec<Ink> {
                staves
                    .iter()
                    .flat_map(|(b, _)| ink_of(system, *b).iter().copied())
                    .collect()
            };
            let s = separation(
                &lowest_bottoms(&gather(upper, &above)),
                &highest_tops(&gather(lower, &below)),
                line_above,
                line_below,
            );
            if let Some(raw) = vertical_unit(line_above - line_below, floor, s, f64::NEG_INFINITY) {
                inter_system.push(raw);
            }
        }
        first += page.systems.len();
    }

    VerticalUnits {
        inter_staff,
        inter_system,
    }
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
    use super::{
        census, held, highest_tops, lowest_bottoms, separation, vertical_units, Ink, VerticalUnits,
        HORIZONTAL_CLEARANCE, LYRIC_REACH,
    };
    use crate::{Engraver, PageGeometry};
    use epiphany_layout_ir::{
        to_constrained, to_logical, ConstrainedLayoutIR, ConstraintSolver, QualityMetricKind,
        QualityMetricVector, SolveStatus, SolverConfig, SolverProfile, SolverWarningKind,
        QUALITY_METRIC_KINDS,
    };

    /// The QUICKSTART ten-measure hand-off fixture: wraps into three systems
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
        // page-fill axis degenerates to exactly 0.0). Its forty quarters,
        // spaced by their duration, break three/four/three, and every system,
        // the last included (its ink fills past three tenths of the width),
        // is justified to the content width.
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
            "the non-final systems fill the width after justification: {}",
            vector.system_break_penalty.0
        );
        // The systems' ink widths are all the content width, so the casting-off
        // axis, their width uniformity, is near zero; their glyph densities
        // still differ, four measures against three.
        assert!(
            vector.casting_off_quality.0 < 0.1,
            "every system spans the content width: {}",
            vector.casting_off_quality.0
        );
        assert!(
            (0.2..0.35).contains(&vector.symbol_density_uniformity.0),
            "density differs between a system of four measures and one of three: {}",
            vector.symbol_density_uniformity.0
        );
        // Neither axis approaches its Standard floor, so no SHOULD-level floor
        // diagnostic fires (per the catalog, one would never change status).
        let floored = |metric: QualityMetricKind| {
            report.warnings.iter().any(|w| {
                matches!(w.kind, SolverWarningKind::QualityFloorApproached { metric: m } if m == metric)
            })
        };
        assert!(!floored(QualityMetricKind::CastingOff));
        assert!(!floored(QualityMetricKind::SystemBreak));
        assert_eq!(report.status, SolveStatus::Solved);
    }

    #[test]
    fn slur_shape_is_measured_penalizing_out_of_band_arcs() {
        use epiphany_core::{CurvatureOverride, Slur, SlurId, SlurKind, SpaceUnit, SpanStyle};
        use epiphany_determinism::CanonicalF64;
        use epiphany_layout_ir::to_constrained;

        let with_slur = |start: usize, end: usize, height: Option<f64>| {
            let mut s = epiphany_testkit::fixtures::ten_measure_single_staff(0x000A_11CE);
            let ev: Vec<_> = s.canvas.regions[0].staff_instances()[0].voices[0]
                .events
                .clone();
            s.cross_cutting.slurs.push(Slur {
                id: s.identity.mint::<SlurId>(),
                start_event: ev[start],
                end_event: ev[end],
                kind: SlurKind::Legato,
                curvature_override: height.map(|h| CurvatureOverride {
                    direction: None,
                    height: Some(SpaceUnit(CanonicalF64::new(h).expect("finite"))),
                }),
                style: SpanStyle::default(),
            });
            Engraver::default()
                .solve(&to_constrained(&to_logical(&s)), &SolverConfig::default())
                .metric_vector
                .slur_shape_penalty
                .0
        };
        // A slur over adjacent quarters authored three spaces tall: a tall arc
        // over a chord of one quarter's column (ρ well above the 0.25 band), a
        // real bulge penalty.
        assert!(
            with_slur(0, 1, Some(3.0)) > 0.0,
            "a bulgy short slur is penalized: {}",
            with_slur(0, 1, Some(3.0))
        );
        // A slur over a wide span: the auto height gives ρ ≈ 0.16, inside the
        // ideal band [0.08, 0.25], so no penalty.
        assert_eq!(
            with_slur(0, 6, None),
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
        // Three events either side of the fixture's first system break (the
        // first system's notes, counted), wide enough that the minimum height
        // does not bind and short enough that the slur stays in band.
        let plain = Engraver::default()
            .solve(
                &to_constrained(&to_logical(&score)),
                &SolverConfig::default(),
            )
            .layout;
        let first_heads = plain
            .systems()
            .next()
            .expect("a system")
            .primitives
            .glyphs
            .iter()
            .filter(|&&i| {
                matches!(
                    plain.glyphs[i as usize].provenance.source,
                    TypedObjectId::Pitch(_)
                )
            })
            .count();
        assert!(first_heads > 3, "the first system holds notes");
        score.cross_cutting.slurs.push(Slur {
            id,
            start_event: ev[first_heads - 3],
            end_event: ev[first_heads + 3],
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
        // note. Pinned so the interaction is not silent (review finding). A
        // page holds two systems, the first with the fixture's rehearsal mark
        // over it, so justification has a gap to spread.
        let geometry = PageGeometry {
            size: Size2D {
                width: StaffSpace(40.0),
                height: StaffSpace(60.0),
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
        // Justification fills the non-final pages, to the borders its first
        // and last staves keep from the margins (X5d.3)…
        assert!(
            report.metric_vector.page_fill_efficiency.0 < 0.3,
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

    /// Runs the real pipeline far enough to hand `vertical_units` a `CastLayout`
    /// — the same spacing + casting-off `Engraver::resolve` performs.
    fn units(score: &epiphany_core::Score) -> VerticalUnits {
        units_of(to_constrained(&to_logical(score)))
    }

    fn units_of(input: ConstrainedLayoutIR) -> VerticalUnits {
        units_on(input, Engraver::default().geometry())
    }

    /// The units of `input` cast on `geometry`, with `bake` applied to the
    /// cast first (a defect the census must see).
    fn units_baked(
        input: ConstrainedLayoutIR,
        geometry: PageGeometry,
        bake: impl FnOnce(&ConstrainedLayoutIR, &mut crate::casting::CastLayout),
    ) -> VerticalUnits {
        let remap = crate::HorizontalRemap::build(&input);
        let mut cast = crate::casting::cast_off(
            &input,
            &remap.glyphs(&input),
            &remap.strokes(&input),
            &remap.curves(&input),
            &remap.texts(&input),
            &crate::casting::Respacing {
                skyline: &remap.skyline,
                curves: &remap.unpassed_curves(&input),
            },
            &geometry,
        );
        bake(&input, &mut cast);
        let census = census(&input, &cast);
        vertical_units(&input, &cast, &census)
    }

    /// A page of no height, which nothing spreads.
    fn unbounded() -> PageGeometry {
        let mut geometry = Engraver::default().geometry();
        geometry.size.height = epiphany_layout_ir::StaffSpace(0.0);
        geometry
    }

    fn units_on(input: ConstrainedLayoutIR, geometry: PageGeometry) -> VerticalUnits {
        units_baked(input, geometry, |_, _| {})
    }

    /// Unspread, the solve stands every gap exactly where its floor or its
    /// ink asks, so each unit reads no deviation: the census, reading the
    /// baked output with code of its own, agrees with the solve.
    #[test]
    fn an_unspread_solve_realizes_what_every_gap_asks() {
        for score in [
            epiphany_testkit::fixtures::two_staff_close_content(1),
            epiphany_testkit::fixtures::three_staff_close_content(1),
            epiphany_testkit::fixtures::two_staff_wrapping_pressure(1),
            epiphany_testkit::fixtures::ten_measure_single_staff(1),
            epiphany_testkit::fixtures::ten_measure_with_slurs(1),
        ] {
            let u = units_on(to_constrained(&to_logical(&score)), unbounded());
            assert!(!u.inter_staff.is_empty() || !u.inter_system.is_empty());
            for value in u.inter_staff.iter().chain(&u.inter_system) {
                assert!(*value < 1e-3, "{:?} {:?}", u.inter_staff, u.inter_system);
            }
        }
        let u = units_on(
            epiphany_testkit::fixtures::percussion_placeholder_constrained(1),
            unbounded(),
        );
        assert!(
            u.inter_staff.iter().all(|v| *v < 1e-3),
            "{:?}",
            u.inter_staff
        );
    }

    /// A staff the bake failed to move reads as crowding: the two-staff
    /// fixture's lower staff, every primitive of its band raised one space
    /// after casting, stands a space nearer than its ink asks.
    #[test]
    fn a_staff_the_bake_left_behind_reads_as_crowding() {
        let score = epiphany_testkit::fixtures::two_staff_close_content(1);
        let input = to_constrained(&to_logical(&score));
        let lower = input
            .vertical_bands
            .iter()
            .filter(|b| matches!(b.kind, epiphany_layout_ir::VerticalBandKind::Staff(_)))
            .map(|b| b.id)
            .nth(1)
            .expect("two staves");
        let clean = units_on(input.clone(), unbounded());
        let moved = units_baked(input, unbounded(), |input, cast| {
            for (g, source) in cast.glyphs.iter_mut().zip(&input.glyphs) {
                if source.vertical_band == lower {
                    g.position =
                        epiphany_layout_ir::Point::new(g.position.x.0, g.position.y.0 + 1.0);
                }
            }
            for s in cast.strokes.iter_mut().filter(|s| s.vertical_band == lower) {
                s.from = epiphany_layout_ir::Point::new(s.from.x.0, s.from.y.0 + 1.0);
                s.to = epiphany_layout_ir::Point::new(s.to.x.0, s.to.y.0 + 1.0);
            }
            for c in cast.curves.iter_mut().filter(|c| c.vertical_band == lower) {
                for p in [&mut c.p0, &mut c.p1, &mut c.p2, &mut c.p3] {
                    *p = epiphany_layout_ir::Point::new(p.x.0, p.y.0 + 1.0);
                }
            }
        });
        assert!(clean.inter_staff[0] < 1e-3, "{:?}", clean.inter_staff);
        // Its ink one space nearer, the gap asks one space more than it has.
        assert!(moved.inter_staff[0] > 0.02, "{:?}", moved.inter_staff);
    }

    /// Inks meet across where they come within a quarter space: 0.2 apart
    /// they do, 0.3 apart they do not.
    #[test]
    fn inks_meet_across_within_a_quarter_space() {
        let above: Vec<Ink> = vec![([0.0, -2.0, 1.0, 0.0], 0.0, 0.0)];
        let near: Vec<Ink> = vec![([1.2, -6.0, 2.0, -3.0], 0.0, 0.0)];
        let far: Vec<Ink> = vec![([1.3, -6.0, 2.0, -3.0], 0.0, 0.0)];
        let s =
            |below: &[Ink]| separation(&lowest_bottoms(&above), &highest_tops(below), 0.0, -4.0);
        assert!((s(&near) - 3.0).abs() < 1e-9, "{}", s(&near));
        assert_eq!(s(&far), f64::NEG_INFINITY);
    }

    /// Ink of a staff two above a gap counts as the staff next above does:
    /// planted past the end of the lines, where the middle staff draws
    /// nothing, the top staff's reach down meets the bottom staff's reach
    /// up, and the gap reads the crowding.
    #[test]
    fn a_gap_reads_every_staff_above_it() {
        let score = epiphany_testkit::fixtures::three_staff_close_content(1);
        let input = to_constrained(&to_logical(&score));
        let staves: Vec<_> = input
            .vertical_bands
            .iter()
            .filter(|b| matches!(b.kind, epiphany_layout_ir::VerticalBandKind::Staff(_)))
            .map(|b| b.id)
            .collect();
        let (top, bottom) = (staves[0], staves[2]);
        let clean = units_on(input.clone(), unbounded());
        let planted = units_baked(input, unbounded(), |_, cast| {
            let line = |band| {
                cast.strokes
                    .iter()
                    .filter(|s| {
                        s.vertical_band == band
                            && matches!(s.provenance.source, epiphany_core::TypedObjectId::Staff(_))
                    })
                    .map(|s| (s.from.y.0, s.to.x.0.max(s.from.x.0), s.clone()))
                    .fold(
                        (f32::INFINITY, f32::NEG_INFINITY, None),
                        |(lo, right, _), (y, x, s)| (lo.min(y), right.max(x), Some(s)),
                    )
            };
            let (top_low, right, top_stroke) = line(top);
            let (bottom_low, _, bottom_stroke) = line(bottom);
            let x = right + 5.0;
            let bottom_top_line = bottom_low + 4.0;
            let mut reach = top_stroke.expect("a top staff line");
            reach.from = epiphany_layout_ir::Point::new(x, top_low);
            reach.to = epiphany_layout_ir::Point::new(x, bottom_top_line - 1.0);
            let mut rise = bottom_stroke.expect("a bottom staff line");
            rise.from = epiphany_layout_ir::Point::new(x, bottom_top_line);
            rise.to = epiphany_layout_ir::Point::new(x, bottom_top_line + 0.5);
            for stroke in [reach, rise] {
                cast.strokes.push(stroke);
                cast.stroke_system.push(Some(0));
            }
        });
        assert!(clean.inter_staff[1] < 1e-3, "{:?}", clean.inter_staff);
        assert!(planted.inter_staff[1] > 0.1, "{:?}", planted.inter_staff);
    }

    /// The separation over intervals equals the greatest over every pair that
    /// meets, on boxes drawn at random.
    #[test]
    fn the_separation_is_the_greatest_over_meeting_pairs() {
        let mut rng = epiphany_determinism::fuzz::SplitMix64::new(0x5d4);
        let mut draw = |lo: f64, span: f64| lo + (rng.next_u64() % 10_000) as f64 / 10_000.0 * span;
        for _ in 0..200 {
            let mut boxes = |n: usize, y: f64| -> Vec<Ink> {
                (0..n)
                    .map(|_| {
                        let (l, b) = (draw(0.0, 20.0), draw(y, 4.0));
                        let (w, h) = (draw(0.0, 3.0), draw(0.0, 3.0));
                        let pad = if draw(0.0, 1.0) < 0.2 {
                            LYRIC_REACH
                        } else {
                            0.0
                        };
                        ([l, b, l + w, b + h], pad, pad)
                    })
                    .collect()
            };
            let (above, below) = (boxes(6, 0.0), boxes(6, -5.0));
            let (line_above, line_below) = (1.0, -3.0);
            let mut naive = f64::NEG_INFINITY;
            for (a, down, _) in &above {
                for (b, _, up) in &below {
                    if held(a)
                        && held(b)
                        && a[2] + HORIZONTAL_CLEARANCE > b[0]
                        && a[0] - HORIZONTAL_CLEARANCE < b[2]
                    {
                        naive = naive.max((line_above - (a[1] - down)) + (b[3] + up - line_below));
                    }
                }
            }
            let swept = separation(
                &lowest_bottoms(&above),
                &highest_tops(&below),
                line_above,
                line_below,
            );
            assert!(
                swept == naive || (swept - naive).abs() < 1e-9,
                "{swept} against {naive}"
            );
        }
    }

    /// A staff band owning no glyphs is still a staff of its region, so its gap
    /// contributes. The percussion placeholder's clef has no bundled glyph and it
    /// carries no notes, leaving its band's `members` (a glyph list) empty while
    /// the band owns its staff-line strokes. Identifying a region's staff bands by
    /// `members` drops it, and the axis loses the unit entirely.
    #[test]
    fn a_glyphless_staff_band_still_contributes_an_inter_staff_unit() {
        let input = epiphany_testkit::fixtures::percussion_placeholder_constrained(1);
        let systems = Engraver::default()
            .solve(&input, &SolverConfig::default())
            .layout
            .systems()
            .count();
        let u = units_of(input);
        assert!(systems >= 2, "the fixture wraps");
        assert_eq!(
            u.inter_staff.len(),
            systems,
            "one unit per system realizing the pair, not zero"
        );
    }

    /// A gap band realized in several systems contributes one unit PER SYSTEM.
    /// The inter-staff solve sizes each system's gaps from that system's own
    /// content, so the realizations are independent measurements; the wrapping
    /// fixture's systems sit at very different staff distances. (Since the
    /// staves are set by their outlines, X5d.2, no solve realizes the whole
    /// extents' clearance this unit reads; the solve's own rule is read back by
    /// `vertical::solve_tests`.)
    #[test]
    fn each_system_realizing_a_gap_band_contributes_its_own_unit() {
        let score = epiphany_testkit::fixtures::two_staff_wrapping_pressure(1);
        let systems = Engraver::default()
            .solve(
                &to_constrained(&to_logical(&score)),
                &SolverConfig::default(),
            )
            .layout
            .systems()
            .count();
        let u = units(&score);
        assert!(systems >= 2, "the fixture wraps");
        assert_eq!(u.inter_staff.len(), systems, "one realization a system");
        // Three staves in one system: two gap bands, one realization each.
        let u = units(&epiphany_testkit::fixtures::three_staff_close_content(1));
        assert_eq!(u.inter_staff.len(), 2, "two gap bands, one system");
    }
}
