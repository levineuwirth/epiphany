//! The render-to-hit-test contract (Chapter 7 §"RenderIR": provenance "is the
//! basis of hit-testing, selection, and back-reference navigation in the UI").
//!
//! A [`RenderIR`] already traces every primitive to its score-graph source; an
//! editor additionally needs a *structured* map from a rendered primitive to its
//! **layout object** and its **score object**, with a selectable **shape**, so a
//! click or drag resolves to something to select without the GUI re-deriving
//! geometry or guessing the provenance chain. [`RenderIR::hit_test_map`] is that
//! map, and it is what gets tested at the RenderIR boundary.
//!
//! ## Coordinate frame
//!
//! Shapes are in **staff-space, y-up world** coordinates — the same frame as
//! [`RenderPrimitive::position`] and stroke endpoints, *before* any
//! renderer's world→screen transform. A GUI maps a screen point to this frame
//! with the inverse of the same transform its renderer uses for display (for the
//! SVG renderer, the inverse of its single `translate(-min_x, max_y) scale(1,-1)`
//! group), then queries the map. The contract is thus resolution- and
//! renderer-independent.

use epiphany_core::TypedObjectId;

use crate::provenance::{LayoutObjectId, SynthesisKind};
use crate::render::{RenderIR, RenderPrimitive};
use crate::spatial::{BoundingBox, Point, Transform2D};

/// Which [`RenderIR`] primitive a [`HitRegion`] belongs to: an index into
/// [`RenderIR::primitives`] (a glyph), [`RenderIR::strokes`] (a stroke), or
/// [`RenderIR::curves`] (a curve).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum PrimitiveRef {
    Glyph(usize),
    Stroke(usize),
    Curve(usize),
}

impl PrimitiveRef {
    /// Whether this is a glyph primitive (vs. a stroke).
    #[inline]
    pub fn is_glyph(self) -> bool {
        matches!(self, PrimitiveRef::Glyph(_))
    }
}

/// A selectable region's shape, in staff-space, y-up world coordinates.
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum HitShape {
    /// A glyph's drawn extent: an axis-aligned world box (its `bounding_box`
    /// placed by `position` and any `transform`).
    Box(BoundingBox),
    /// A stroke (staff line, stem, barline, …): a line segment with a half-width
    /// (half the stroke thickness), so a click near the line still selects it.
    Segment {
        from: Point,
        to: Point,
        half_width: f32,
    },
    /// A curve (slur, …): its four cubic-bézier control points and a half-width.
    /// Its geometry tests flatten the cubic into [`CURVE_FLATTEN_SEGMENTS`]
    /// straight capsule segments (a polyline), inflated by a conservative
    /// flattening-error bound so a click on the true drawn arc — which can bow
    /// outside the sampled chords for a thin, high-curvature slur — never
    /// misses. One region per curve, unlike per-segment hit fragments.
    Curve {
        p0: Point,
        p1: Point,
        p2: Point,
        p3: Point,
        half_width: f32,
    },
}

/// How many straight segments a cubic bézier is flattened into for hit-testing.
/// Chosen so a normal-span slur's chord error stays well under the half-width;
/// a fixed count keeps the map deterministic and cheap.
pub const CURVE_FLATTEN_SEGMENTS: usize = 16;

/// The cubic-bézier point at parameter `t` (de Casteljau, expanded).
fn cubic_point(p0: Point, p1: Point, p2: Point, p3: Point, t: f32) -> Point {
    let u = 1.0 - t;
    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    Point::new(
        a * p0.x.0 + b * p1.x.0 + c * p2.x.0 + d * p3.x.0,
        a * p0.y.0 + b * p1.y.0 + c * p2.y.0 + d * p3.y.0,
    )
}

/// The flattened polyline of a cubic bézier: `CURVE_FLATTEN_SEGMENTS + 1`
/// points from `p0` to `p3`, endpoints exact.
fn flatten_cubic(p0: Point, p1: Point, p2: Point, p3: Point) -> Vec<Point> {
    (0..=CURVE_FLATTEN_SEGMENTS)
        .map(|i| {
            let t = i as f32 / CURVE_FLATTEN_SEGMENTS as f32;
            cubic_point(p0, p1, p2, p3, t)
        })
        .collect()
}

/// A **conservative** upper bound on the maximum distance between the true cubic
/// and its [`flatten_cubic`] polyline. The chord error over a parameter
/// interval of length `h` is at most `h²/8 · max‖B''‖`; a cubic's second
/// derivative is linear, so `max‖B''‖ = 6 · max(‖p0−2p1+p2‖, ‖p1−2p2+p3‖)`
/// (attained at an endpoint), and `h = 1/N`. The [`HitShape::Curve`] tests
/// inflate the capsule half-width by this bound so a click on the *drawn* arc —
/// which can bow outside the flattened chords for a thin, high-curvature slur —
/// never misses (the true curve lies within this distance of the polyline).
fn flatten_error_bound(p0: Point, p1: Point, p2: Point, p3: Point) -> f32 {
    let second_diff = |a: Point, b: Point, c: Point| {
        let dx = a.x.0 - 2.0 * b.x.0 + c.x.0;
        let dy = a.y.0 - 2.0 * b.y.0 + c.y.0;
        (dx * dx + dy * dy).sqrt()
    };
    let d = second_diff(p0, p1, p2).max(second_diff(p1, p2, p3));
    let n = CURVE_FLATTEN_SEGMENTS as f32;
    // h²/8 · 6·d = (3/(4N²))·d.
    (3.0 * d) / (4.0 * n * n)
}

impl HitShape {
    /// Whether a world `point` lies within this shape — the click-selection test.
    /// A box is closed (edges included); a segment is within `half_width` of the
    /// line, so thin strokes remain clickable.
    pub fn contains(&self, point: Point) -> bool {
        match self {
            HitShape::Box(b) => box_contains(b, point),
            HitShape::Segment {
                from,
                to,
                half_width,
            } => distance_point_segment(point, *from, *to) <= *half_width,
            HitShape::Curve {
                p0,
                p1,
                p2,
                p3,
                half_width,
            } => {
                // Inflate by the flattening-error bound so ink that bows outside
                // the sampled chords is still hit (see `flatten_error_bound`).
                let reach = *half_width + flatten_error_bound(*p0, *p1, *p2, *p3);
                flatten_cubic(*p0, *p1, *p2, *p3)
                    .windows(2)
                    .any(|seg| distance_point_segment(point, seg[0], seg[1]) <= reach)
            }
        }
    }

    /// Whether this shape **exactly** intersects an axis-aligned world `rect` — the
    /// drag/rubber-band selection test. A box's overlap is exact; a segment's is a
    /// true capsule-vs-rectangle test (its half-width included), so a diagonal
    /// stroke whose *bounding box* clips a corner of `rect` is not falsely selected.
    /// The shape's [`Self::aabb`] is used internally as a broad-phase reject.
    pub fn intersects_rect(&self, rect: BoundingBox) -> bool {
        if !boxes_overlap(&self.aabb(), &rect) {
            return false; // broad-phase reject
        }
        match self {
            // A box equals its AABB, so the broad-phase overlap above was exact.
            HitShape::Box(_) => true,
            HitShape::Segment {
                from,
                to,
                half_width,
            } => segment_intersects_rect(*from, *to, *half_width, &rect),
            HitShape::Curve {
                p0,
                p1,
                p2,
                p3,
                half_width,
            } => {
                let reach = *half_width + flatten_error_bound(*p0, *p1, *p2, *p3);
                flatten_cubic(*p0, *p1, *p2, *p3)
                    .windows(2)
                    .any(|seg| segment_intersects_rect(seg[0], seg[1], reach, &rect))
            }
        }
    }

    /// The shape's axis-aligned world bounding box — the broad-phase rectangle for
    /// drag/rubber-band selection (see [`HitTestMap::within`]).
    pub fn aabb(&self) -> BoundingBox {
        match self {
            HitShape::Box(b) => *b,
            HitShape::Segment {
                from,
                to,
                half_width,
            } => BoundingBox::new(
                from.x.0.min(to.x.0) - half_width,
                from.y.0.min(to.y.0) - half_width,
                from.x.0.max(to.x.0) + half_width,
                from.y.0.max(to.y.0) + half_width,
            ),
            HitShape::Curve {
                p0,
                p1,
                p2,
                p3,
                half_width,
            } => {
                // A cubic lies within its control points' convex hull, so their
                // AABB (± half-width) is a correct conservative broad-phase box.
                let xs = [p0.x.0, p1.x.0, p2.x.0, p3.x.0];
                let ys = [p0.y.0, p1.y.0, p2.y.0, p3.y.0];
                let min = |a: &[f32]| a.iter().copied().fold(f32::INFINITY, f32::min);
                let max = |a: &[f32]| a.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                BoundingBox::new(
                    min(&xs) - half_width,
                    min(&ys) - half_width,
                    max(&xs) + half_width,
                    max(&ys) + half_width,
                )
            }
        }
    }
}

/// One hit-test region: a selectable [`HitShape`] plus the provenance chain an
/// editor resolves a click or drag to — the **rendered primitive**, its **layout
/// object** (stable across relayout), and its **score object**.
#[derive(Clone, PartialEq, Debug)]
pub struct HitRegion {
    /// The rendered primitive this region covers.
    pub primitive: PrimitiveRef,
    /// The score-graph object to select when this region is hit
    /// (`provenance.source`).
    pub source: TypedObjectId,
    /// The layout object the primitive manifests (`provenance.stable_id`), stable
    /// across re-layouts of an unchanged source — the right anchor for a cursor or
    /// a persistent selection that must survive a relayout.
    pub layout_object: LayoutObjectId,
    /// Set when the primitive is engraver-synthesized (no direct score-graph
    /// manifestation), so an editor can treat a generated object specially (e.g.
    /// select its source rather than the synthesized mark).
    pub synthesis: Option<SynthesisKind>,
    /// The selectable shape, in world coordinates.
    pub shape: HitShape,
    /// The draw layer, used to break ties when regions overlap (a higher layer, or
    /// a glyph over a stroke at the same layer, is "on top").
    pub layer: i32,
}

impl HitRegion {
    /// A z-order key matching the renderer's paint order (layer ascending, strokes
    /// before glyphs at one layer, then primitive index). A larger key is painted
    /// later, i.e. on top.
    fn paint_order(&self) -> (i32, u8, usize) {
        // The renderer paints strokes, then curves, then glyphs at one layer
        // (a slur draws over the staff lines but under the noteheads it joins).
        let (kind_rank, index) = match self.primitive {
            PrimitiveRef::Stroke(i) => (0, i),
            PrimitiveRef::Curve(i) => (1, i),
            PrimitiveRef::Glyph(i) => (2, i),
        };
        (self.layer, kind_rank, index)
    }
}

/// The hit-test map over a [`RenderIR`]: one [`HitRegion`] per primitive (glyph,
/// stroke, and curve). The public [`Self::regions`] vector is stored in
/// construction order (glyph regions, then stroke regions, then curve
/// regions), not z-order; callers that need ordered selection results should
/// use [`Self::hit`] or [`Self::within`].
#[derive(Clone, PartialEq, Debug)]
pub struct HitTestMap {
    pub regions: Vec<HitRegion>,
}

impl HitTestMap {
    /// Every region whose shape contains `point`, **topmost first** (reverse paint
    /// order). The first element is what a single-selection click should pick; the
    /// rest support cycling through stacked objects.
    pub fn hit(&self, point: Point) -> Vec<&HitRegion> {
        let mut hits: Vec<&HitRegion> = self
            .regions
            .iter()
            .filter(|r| r.shape.contains(point))
            .collect();
        // Topmost first: descending paint order (later-painted = on top).
        hits.sort_by_key(|r| std::cmp::Reverse(r.paint_order()));
        hits
    }

    /// Every region whose shape **exactly** intersects `rect`, in **ascending paint
    /// order** (back-to-front — the renderer's draw order: layer ascending, strokes
    /// before glyphs at one layer, then index) — the drag/rubber-band selection
    /// result (see [`HitShape::intersects_rect`]). This is the reverse of
    /// [`Self::hit`]'s topmost-first order, matching the two queries' uses (a click
    /// picks the top object; a drag enumerates a set in draw order). For a cheaper
    /// broad-phase pass, a caller can test [`HitShape::aabb`] directly.
    pub fn within(&self, rect: BoundingBox) -> Vec<&HitRegion> {
        let mut selected: Vec<&HitRegion> = self
            .regions
            .iter()
            .filter(|r| r.shape.intersects_rect(rect))
            .collect();
        selected.sort_by_key(|r| r.paint_order());
        selected
    }
}

impl RenderIR {
    /// Builds the [`HitTestMap`]: one [`HitRegion`] per glyph primitive, per
    /// stroke, and per curve. Regions are stored in construction order (glyphs,
    /// then strokes, then curves); each region's [`HitRegion::layer`] and
    /// primitive reference carry the true paint order consumed by
    /// [`HitTestMap::hit`] and [`HitTestMap::within`].
    /// Each region's `source`/`layout_object`/`synthesis` come straight from the
    /// primitive's preserved [`crate::Provenance`]; its shape is computed in world
    /// coordinates.
    pub fn hit_test_map(&self) -> HitTestMap {
        let mut regions =
            Vec::with_capacity(self.primitives.len() + self.strokes.len() + self.curves.len());
        for (i, p) in self.primitives.iter().enumerate() {
            regions.push(HitRegion {
                primitive: PrimitiveRef::Glyph(i),
                source: p.provenance.source,
                layout_object: p.provenance.stable_id,
                synthesis: p.provenance.synthesis,
                shape: HitShape::Box(glyph_world_box(p)),
                layer: p.layer,
            });
        }
        for (i, s) in self.strokes.iter().enumerate() {
            regions.push(HitRegion {
                primitive: PrimitiveRef::Stroke(i),
                source: s.provenance.source,
                layout_object: s.provenance.stable_id,
                synthesis: s.provenance.synthesis,
                shape: HitShape::Segment {
                    from: s.from,
                    to: s.to,
                    half_width: s.thickness.0 / 2.0,
                },
                layer: s.layer,
            });
        }
        for (i, c) in self.curves.iter().enumerate() {
            regions.push(HitRegion {
                primitive: PrimitiveRef::Curve(i),
                source: c.provenance.source,
                layout_object: c.provenance.stable_id,
                synthesis: c.provenance.synthesis,
                shape: HitShape::Curve {
                    p0: c.p0,
                    p1: c.p1,
                    p2: c.p2,
                    p3: c.p3,
                    half_width: c.thickness.0 / 2.0,
                },
                layer: c.layer,
            });
        }
        HitTestMap { regions }
    }
}

/// A glyph's world-space bounding box: its local `bounding_box`, placed by
/// `position` and any `transform`. The four corners are mapped through the same
/// `translate(position) ∘ transform` the renderer applies (an affine transform may
/// rotate/scale the box past its axis-aligned local extent), then the
/// axis-aligned hull is taken.
fn glyph_world_box(p: &RenderPrimitive) -> BoundingBox {
    let bb = p.bounding_box;
    let (px, py) = (p.position.x.0, p.position.y.0);
    let corners = [
        (bb.left.0, bb.bottom.0),
        (bb.left.0, bb.top.0),
        (bb.right.0, bb.bottom.0),
        (bb.right.0, bb.top.0),
    ];
    let (mut min_x, mut min_y) = (f32::INFINITY, f32::INFINITY);
    let (mut max_x, mut max_y) = (f32::NEG_INFINITY, f32::NEG_INFINITY);
    for (lx, ly) in corners {
        let (wx, wy) = placed(px, py, &p.transform, lx, ly);
        min_x = min_x.min(wx);
        min_y = min_y.min(wy);
        max_x = max_x.max(wx);
        max_y = max_y.max(wy);
    }
    BoundingBox::new(min_x, min_y, max_x, max_y)
}

/// Maps a glyph-local point `(lx, ly)` to world coordinates through the glyph's
/// optional `transform` and its `position` translate — identical to the placement
/// the renderer applies, so a hit region aligns with the drawn glyph.
fn placed(px: f32, py: f32, transform: &Option<Transform2D>, lx: f32, ly: f32) -> (f32, f32) {
    let (tx, ty) = match transform {
        None => (lx, ly),
        Some(t) => {
            let m = t.matrix;
            (
                m[0][0] * lx + m[0][1] * ly + m[0][2],
                m[1][0] * lx + m[1][1] * ly + m[1][2],
            )
        }
    };
    (px + tx, py + ty)
}

/// The Euclidean distance from `p` to the segment `a`–`b` (a degenerate segment is
/// a point).
fn distance_point_segment(p: Point, a: Point, b: Point) -> f32 {
    let (px, py) = (p.x.0, p.y.0);
    let (ax, ay) = (a.x.0, a.y.0);
    let (bx, by) = (b.x.0, b.y.0);
    let (dx, dy) = (bx - ax, by - ay);
    let len_sq = dx * dx + dy * dy;
    // Project p onto the segment, clamping the parameter to [0, 1].
    let t = if len_sq <= f32::EPSILON {
        0.0
    } else {
        (((px - ax) * dx + (py - ay) * dy) / len_sq).clamp(0.0, 1.0)
    };
    let (cx, cy) = (ax + t * dx, ay + t * dy);
    ((px - cx).powi(2) + (py - cy).powi(2)).sqrt()
}

/// Whether two axis-aligned world boxes overlap (touching edges count).
fn boxes_overlap(a: &BoundingBox, b: &BoundingBox) -> bool {
    a.left.0 <= b.right.0 && a.right.0 >= b.left.0 && a.bottom.0 <= b.top.0 && a.top.0 >= b.bottom.0
}

/// Whether a world `point` lies within the closed box `b` (edges included).
fn box_contains(b: &BoundingBox, point: Point) -> bool {
    point.x.0 >= b.left.0
        && point.x.0 <= b.right.0
        && point.y.0 >= b.bottom.0
        && point.y.0 <= b.top.0
}

/// Whether a thick segment (capsule: the segment `from`–`to` grown by
/// `half_width`) intersects the axis-aligned `rect`. Exact: a segment whose
/// *bounding box* clips the rect but whose body misses it is correctly rejected.
fn segment_intersects_rect(from: Point, to: Point, half_width: f32, rect: &BoundingBox) -> bool {
    // An endpoint inside the rect ⇒ the body touches the rect's interior.
    if box_contains(rect, from) || box_contains(rect, to) {
        return true;
    }
    // Otherwise the body is within `half_width` of the rect iff it is within
    // `half_width` of one of the four edges (distance 0 means it crosses one).
    let c = [
        Point::new(rect.left.0, rect.bottom.0),
        Point::new(rect.right.0, rect.bottom.0),
        Point::new(rect.right.0, rect.top.0),
        Point::new(rect.left.0, rect.top.0),
    ];
    (0..4).any(|i| segment_segment_distance(from, to, c[i], c[(i + 1) % 4]) <= half_width)
}

/// The minimum Euclidean distance between two 2-D segments (0 if they cross).
fn segment_segment_distance(a1: Point, a2: Point, b1: Point, b2: Point) -> f32 {
    if segments_cross(a1, a2, b1, b2) {
        return 0.0;
    }
    distance_point_segment(a1, b1, b2)
        .min(distance_point_segment(a2, b1, b2))
        .min(distance_point_segment(b1, a1, a2))
        .min(distance_point_segment(b2, a1, a2))
}

/// Whether segments `p1`–`p2` and `p3`–`p4` intersect (proper crossing or a
/// collinear touch), by the standard orientation test.
fn segments_cross(p1: Point, p2: Point, p3: Point, p4: Point) -> bool {
    fn orient(a: Point, b: Point, c: Point) -> f32 {
        (b.x.0 - a.x.0) * (c.y.0 - a.y.0) - (b.y.0 - a.y.0) * (c.x.0 - a.x.0)
    }
    // Whether collinear point `c` lies within the bounding box of `a`–`b`.
    fn on_segment(a: Point, b: Point, c: Point) -> bool {
        c.x.0 >= a.x.0.min(b.x.0)
            && c.x.0 <= a.x.0.max(b.x.0)
            && c.y.0 >= a.y.0.min(b.y.0)
            && c.y.0 <= a.y.0.max(b.y.0)
    }
    let (d1, d2) = (orient(p3, p4, p1), orient(p3, p4, p2));
    let (d3, d4) = (orient(p1, p2, p3), orient(p1, p2, p4));
    if ((d1 > 0.0) != (d2 > 0.0)) && ((d3 > 0.0) != (d4 > 0.0)) && d1 != 0.0 && d3 != 0.0 {
        return true;
    }
    (d1 == 0.0 && on_segment(p3, p4, p1))
        || (d2 == 0.0 && on_segment(p3, p4, p2))
        || (d3 == 0.0 && on_segment(p1, p2, p3))
        || (d4 == 0.0 && on_segment(p1, p2, p4))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constrained::{to_constrained, Curve};
    use crate::logical::to_logical;
    use crate::provenance::Provenance;
    use crate::render::to_render;
    use crate::solver::{ConstraintSolver, SolverConfig, StubSolver};
    use crate::spatial::StaffSpace;
    use crate::{GlyphReference, GlyphStyle, Stroke};
    use epiphany_core::{EventId, StaffId};

    fn render_of(seed: u64) -> RenderIR {
        let constrained = to_constrained(&to_logical(
            &epiphany_core::generators::valid_score_rich(seed),
        ));
        let resolved = StubSolver
            .solve(&constrained, &SolverConfig::default())
            .layout;
        to_render(&resolved)
    }

    fn glyph(position: Point, bbox: BoundingBox, layer: i32) -> RenderPrimitive {
        RenderPrimitive {
            provenance: Provenance::projected(
                TypedObjectId::Event(EventId::from_raw(layer as u128 + 1)),
                vec![],
            ),
            glyph: GlyphReference::borrowed("noteheadBlack"),
            position,
            transform: None,
            bounding_box: bbox,
            style: GlyphStyle::default(),
            layer,
        }
    }

    fn stroke(from: Point, to: Point, layer: i32) -> Stroke {
        Stroke {
            provenance: Provenance::projected(TypedObjectId::Staff(StaffId::from_raw(1)), vec![]),
            vertical_band: crate::VerticalBandId(0),
            from,
            to,
            thickness: StaffSpace(0.2),
            layer,
            style: GlyphStyle::default(),
        }
    }

    #[test]
    fn a_box_region_is_closed_and_its_aabb_is_itself() {
        let s = HitShape::Box(BoundingBox::new(-1.0, -0.5, 1.0, 0.5));
        assert!(s.contains(Point::new(0.0, 0.0))); // centre
        assert!(s.contains(Point::new(-1.0, -0.5))); // corner included (closed)
        assert!(s.contains(Point::new(1.0, 0.5)));
        assert!(!s.contains(Point::new(1.01, 0.0))); // just past the right edge
        assert!(!s.contains(Point::new(0.0, 0.6))); // just past the top
        assert_eq!(s.aabb(), BoundingBox::new(-1.0, -0.5, 1.0, 0.5));
    }

    #[test]
    fn a_segment_region_is_hit_within_its_half_width() {
        // A horizontal stroke (0,0)->(4,0), thickness 0.2 => half-width 0.1.
        let s = HitShape::Segment {
            from: Point::new(0.0, 0.0),
            to: Point::new(4.0, 0.0),
            half_width: 0.1,
        };
        assert!(s.contains(Point::new(2.0, 0.0))); // on the line
        assert!(s.contains(Point::new(2.0, 0.09))); // within the half-width
        assert!(!s.contains(Point::new(2.0, 0.2))); // beyond it
        assert!(!s.contains(Point::new(5.0, 0.0))); // past the endpoint
        assert!(s.contains(Point::new(0.0, 0.05))); // near an endpoint, within
                                                    // The aabb expands by the half-width on every side.
        assert_eq!(s.aabb(), BoundingBox::new(-0.1, -0.1, 4.1, 0.1));
    }

    #[test]
    fn a_curve_region_is_hit_near_its_flattened_arc_not_its_chord() {
        // A symmetric arc bulging up: endpoints (0,0)->(4,0), controls lifted
        // to y = 2 so the apex sits at 0.75·2 = 1.5. Half-width 0.1.
        let s = HitShape::Curve {
            p0: Point::new(0.0, 0.0),
            p1: Point::new(1.0, 2.0),
            p2: Point::new(3.0, 2.0),
            p3: Point::new(4.0, 0.0),
            half_width: 0.1,
        };
        // A point on the drawn arc near its apex is hit…
        assert!(s.contains(Point::new(2.0, 1.5)));
        // …but the chord midpoint (y=0, far below the arc) is NOT — a curve is
        // its flattened polyline, not the straight line between its endpoints.
        assert!(!s.contains(Point::new(2.0, 0.0)));
        // The endpoints are exact.
        assert!(s.contains(Point::new(0.0, 0.0)));
        assert!(s.contains(Point::new(4.0, 0.0)));
        // The broad-phase AABB is the control hull ± half-width.
        assert_eq!(s.aabb(), BoundingBox::new(-0.1, -0.1, 4.1, 2.1));
        // …and a rubber-band rect over the apex selects it.
        assert!(s.intersects_rect(BoundingBox::new(1.5, 1.3, 2.5, 1.7)));
    }

    #[test]
    fn a_thin_high_curvature_curve_is_hit_on_its_true_arc_between_samples() {
        // A near-degenerate-thin curve (half_width 0.001) with strong curvature:
        // a point ON the true cubic strictly between two flatten samples bows
        // outside the chord capsule at that half-width, but the error-bound
        // inflation still hits it.
        let (p0, p1, p2, p3) = (
            Point::new(0.0, 0.0),
            Point::new(0.0, 6.0),
            Point::new(6.0, 6.0),
            Point::new(6.0, 0.0),
        );
        let s = HitShape::Curve {
            p0,
            p1,
            p2,
            p3,
            half_width: 0.001,
        };
        // A true-cubic point at a parameter offset from the 1/16 grid (t = 1/32
        // falls between samples 0 and 1).
        let on_arc = cubic_point(p0, p1, p2, p3, 1.0 / 32.0);
        assert!(
            s.contains(on_arc),
            "a point on the drawn arc between flatten samples must be hit"
        );
        // The error bound is positive for a genuinely curved shape and zero for
        // collinear control points (a straight "curve").
        assert!(flatten_error_bound(p0, p1, p2, p3) > 0.0);
        assert_eq!(
            flatten_error_bound(
                Point::new(0.0, 0.0),
                Point::new(1.0, 0.0),
                Point::new(2.0, 0.0),
                Point::new(3.0, 0.0),
            ),
            0.0
        );
    }

    #[test]
    fn a_curve_becomes_one_hit_region_tracing_its_source() {
        use epiphany_core::{SlurId, TypedObjectId};
        let slur = SlurId::new(epiphany_core::ReplicaId(3), 9);
        let curve = Curve {
            provenance: Provenance::projected(TypedObjectId::Slur(slur), vec![]),
            vertical_band: crate::VerticalBandId(0),
            p0: Point::new(0.0, 0.0),
            p1: Point::new(1.0, 2.0),
            p2: Point::new(3.0, 2.0),
            p3: Point::new(4.0, 0.0),
            thickness: crate::StaffSpace(0.2),
            layer: 0,
            style: crate::GlyphStyle { rgba: 0 },
            line: epiphany_core::LineStyle::Solid,
        };
        let map = RenderIR {
            primitives: vec![],
            strokes: vec![],
            curves: vec![curve],
        }
        .hit_test_map();
        assert_eq!(map.regions.len(), 1, "one region per curve");
        let region = &map.regions[0];
        assert!(matches!(region.primitive, PrimitiveRef::Curve(0)));
        assert_eq!(region.source, TypedObjectId::Slur(slur));
        // A click on the arc resolves to the slur.
        let hit = map.hit(Point::new(2.0, 1.5));
        assert_eq!(
            hit.first().map(|r| r.source),
            Some(TypedObjectId::Slur(slur))
        );
    }

    #[test]
    fn a_glyph_world_box_is_its_local_box_placed_by_position_and_transform() {
        // No transform: the local box just shifts by the position.
        let p = glyph(
            Point::new(10.0, 3.0),
            BoundingBox::new(-0.5, -0.5, 0.5, 0.5),
            0,
        );
        let HitShape::Box(b) = RenderIR {
            primitives: vec![p.clone()],
            strokes: vec![],
            curves: vec![],
        }
        .hit_test_map()
        .regions[0]
            .shape
        else {
            panic!("glyph region is a box");
        };
        assert_eq!(b, BoundingBox::new(9.5, 2.5, 10.5, 3.5));

        // An affine transform (scale x by 2) rotates/scales the box past its local
        // axis-aligned extent; the hull is taken over the mapped corners.
        let mut t = p;
        t.transform = Some(Transform2D {
            matrix: [[2.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        });
        let HitShape::Box(b) = RenderIR {
            primitives: vec![t],
            strokes: vec![],
            curves: vec![],
        }
        .hit_test_map()
        .regions[0]
            .shape
        else {
            panic!("box");
        };
        assert_eq!(b, BoundingBox::new(9.0, 2.5, 11.0, 3.5));
    }

    #[test]
    fn the_map_covers_every_primitive_and_preserves_the_provenance_chain() {
        let render = render_of(0x5EED);
        let map = render.hit_test_map();
        // One region per glyph and per stroke, none dropped or invented.
        assert_eq!(
            map.regions.len(),
            render.primitives.len() + render.strokes.len() + render.curves.len()
        );
        assert!(!map.regions.is_empty());

        // Each region carries exactly the primitive's preserved chain: rendered
        // primitive -> layout object (stable_id) -> score object (source).
        for r in &map.regions {
            let (source, stable_id, synthesis) = match r.primitive {
                PrimitiveRef::Glyph(i) => {
                    let p = &render.primitives[i];
                    (
                        p.provenance.source,
                        p.provenance.stable_id,
                        p.provenance.synthesis,
                    )
                }
                PrimitiveRef::Stroke(i) => {
                    let s = &render.strokes[i];
                    (
                        s.provenance.source,
                        s.provenance.stable_id,
                        s.provenance.synthesis,
                    )
                }
                PrimitiveRef::Curve(i) => {
                    let c = &render.curves[i];
                    (
                        c.provenance.source,
                        c.provenance.stable_id,
                        c.provenance.synthesis,
                    )
                }
            };
            assert_eq!(r.source, source);
            assert_eq!(r.layout_object, stable_id);
            assert_eq!(r.synthesis, synthesis);
        }
    }

    #[test]
    fn a_click_on_a_notehead_resolves_to_its_pitch() {
        let render = render_of(0x5EED);
        let map = render.hit_test_map();
        let (i, p) = render
            .primitives
            .iter()
            .enumerate()
            .find(|(_, p)| p.glyph.as_str().starts_with("notehead"))
            .expect("the rich fixture renders a notehead");
        let region = map
            .regions
            .iter()
            .find(|r| r.primitive == PrimitiveRef::Glyph(i))
            .unwrap();

        // The full chain an editor needs from one click.
        assert_eq!(region.source, p.provenance.source);
        assert_eq!(region.layout_object, p.provenance.stable_id);
        assert!(
            matches!(region.source, TypedObjectId::Pitch(_)),
            "a notehead's score object is a Pitch, got {:?}",
            region.source
        );

        // A click at the notehead's centre hits its region (a stem stroke may pass
        // through too, but the notehead is among the hits).
        let HitShape::Box(b) = region.shape else {
            panic!("a glyph region is a box");
        };
        let centre = Point::new((b.left.0 + b.right.0) / 2.0, (b.bottom.0 + b.top.0) / 2.0);
        assert!(
            map.hit(centre)
                .iter()
                .any(|h| h.primitive == PrimitiveRef::Glyph(i)),
            "the notehead is hit at its own centre"
        );
    }

    #[test]
    fn overlapping_regions_are_returned_topmost_first() {
        // A stroke and a glyph overlap at the origin; at one layer the glyph paints
        // over the stroke, so it is the topmost hit. A second glyph on a higher
        // layer outranks both.
        let render = RenderIR {
            primitives: vec![
                glyph(Point::ORIGIN, BoundingBox::new(-1.0, -1.0, 1.0, 1.0), 0),
                glyph(Point::ORIGIN, BoundingBox::new(-1.0, -1.0, 1.0, 1.0), 5),
            ],
            strokes: vec![stroke(Point::new(-2.0, 0.0), Point::new(2.0, 0.0), 0)],
            curves: vec![],
        };
        let map = render.hit_test_map();
        let hits = map.hit(Point::ORIGIN);
        assert_eq!(hits.len(), 3, "all three overlap the origin");
        // Topmost: the layer-5 glyph, then the layer-0 glyph (glyph over stroke at a
        // shared layer), then the layer-0 stroke.
        assert_eq!(hits[0].primitive, PrimitiveRef::Glyph(1));
        assert_eq!(hits[1].primitive, PrimitiveRef::Glyph(0));
        assert_eq!(hits[2].primitive, PrimitiveRef::Stroke(0));
    }

    #[test]
    fn within_selects_every_region_intersecting_a_drag_rect() {
        let render = RenderIR {
            primitives: vec![
                glyph(
                    Point::new(0.0, 0.0),
                    BoundingBox::new(-0.5, -0.5, 0.5, 0.5),
                    0,
                ),
                glyph(
                    Point::new(10.0, 0.0),
                    BoundingBox::new(-0.5, -0.5, 0.5, 0.5),
                    0,
                ),
            ],
            strokes: vec![stroke(Point::new(0.0, 0.0), Point::new(3.0, 0.0), 0)],
            curves: vec![],
        };
        let map = render.hit_test_map();
        // A rubber-band around the first glyph and the stroke, but not the far glyph.
        let selected = map.within(BoundingBox::new(-1.0, -1.0, 4.0, 1.0));
        let picked: Vec<_> = selected.iter().map(|r| r.primitive).collect();
        // Both the near glyph and the stroke are selected; the far glyph is not.
        // The result is in ascending paint order — at layer 0 the stroke (drawn
        // first) precedes the glyph, even though the map stores glyphs before
        // strokes.
        assert_eq!(
            picked,
            vec![PrimitiveRef::Stroke(0), PrimitiveRef::Glyph(0)]
        );
    }

    #[test]
    fn within_is_exact_not_just_bounding_box_overlap() {
        // A diagonal stroke (0,0)->(10,10): its AABB is the whole [0,0,10,10]
        // square, which overlaps a small rect at the top-left corner — but the
        // stroke's body never goes near there, so an exact `within` must reject it.
        let render = RenderIR {
            primitives: vec![],
            strokes: vec![stroke(Point::new(0.0, 0.0), Point::new(10.0, 10.0), 0)],
            curves: vec![],
        };
        let map = render.hit_test_map();
        // AABB-overlapping but body-missing rect near (0, 10): rejected.
        assert!(
            map.within(BoundingBox::new(0.0, 9.0, 1.0, 10.0)).is_empty(),
            "a diagonal stroke whose AABB clips a corner is not falsely selected"
        );
        // A rect the stroke actually passes through: selected.
        assert_eq!(
            map.within(BoundingBox::new(4.0, 4.0, 6.0, 6.0)).len(),
            1,
            "a rect the stroke's body crosses selects it"
        );
        // A rect that does not cross the line but lies within the stroke's
        // half-width of it (nearest corner ≈ 0.035 < 0.1): selected via the capsule.
        assert_eq!(
            map.within(BoundingBox::new(5.15, 5.0, 5.25, 5.1)).len(),
            1,
            "a rect within the stroke's half-width selects it"
        );
    }
}
