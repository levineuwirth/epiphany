//! The tight axis-aligned bounding box of a typed outline (Editor T4-pre W2
//! test g4): used only to prove the bundled outlines' drawn ink is contained
//! by their declared `BRAVURA_METRICS` bounding box — the cross-table
//! consistency `layout-ir/src/glyph.rs:107-109` asserts in prose ("the
//! reserved advances/bboxes and the drawn ink agree") but no test in the
//! tree checked directly from parsed outline geometry before this packet.

use epiphany_layout_ir::PathCommand;

/// The exact bounding box `[left, bottom, right, top]` of the ink a sequence
/// of typed path commands draws, computed from real cubic-bezier extrema —
/// not just control points, which routinely lie outside a curve's own tight
/// bounds — matching what `tools/extract_bravura_outlines.py`'s
/// `fontTools.pens.boundsPen.BoundsPen` computes at generation time (it
/// overrides `curveToOne` with the same real-extrema calculation, unlike its
/// `ControlBoundsPen` base class). Returns `None` for an empty command list.
pub(crate) fn outline_extent(commands: &[PathCommand]) -> Option<[f32; 4]> {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let mut cur = (0.0f32, 0.0f32);
    let mut any = false;
    for cmd in commands {
        match cmd {
            PathCommand::MoveTo(p) | PathCommand::LineTo(p) => {
                let (x, y) = (p.x.0, p.y.0);
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
                cur = (x, y);
                any = true;
            }
            PathCommand::CurveTo {
                control1,
                control2,
                to,
            } => {
                let (x0, y0) = cur;
                let (lo_x, hi_x) = cubic_extrema_1d(x0, control1.x.0, control2.x.0, to.x.0);
                let (lo_y, hi_y) = cubic_extrema_1d(y0, control1.y.0, control2.y.0, to.y.0);
                min_x = min_x.min(lo_x);
                max_x = max_x.max(hi_x);
                min_y = min_y.min(lo_y);
                max_y = max_y.max(hi_y);
                any = true;
                cur = (to.x.0, to.y.0);
            }
            PathCommand::Close => {}
        }
    }
    if any {
        Some([min_x, min_y, max_x, max_y])
    } else {
        None
    }
}

/// The `[min, max]` extent of a 1-D cubic bezier `p0..p3` over `t in [0,1]`:
/// the two endpoints plus any interior critical point of the derivative (a
/// root of the quadratic `B'(t)/3 = a*t^2 + b*t + c`).
fn cubic_extrema_1d(p0: f32, p1: f32, p2: f32, p3: f32) -> (f32, f32) {
    let mut lo = p0.min(p3);
    let mut hi = p0.max(p3);
    let d0 = p1 - p0;
    let d1 = p2 - p1;
    let d2 = p3 - p2;
    let a = d0 - 2.0 * d1 + d2;
    let b = 2.0 * (d1 - d0);
    let c = d0;
    let mut consider = |t: f32| {
        if (0.0..=1.0).contains(&t) {
            let mt = 1.0 - t;
            let v =
                mt * mt * mt * p0 + 3.0 * mt * mt * t * p1 + 3.0 * mt * t * t * p2 + t * t * t * p3;
            lo = lo.min(v);
            hi = hi.max(v);
        }
    };
    if a.abs() < 1e-12 {
        if b.abs() > 1e-12 {
            consider(-c / b);
        }
    } else {
        let disc = b * b - 4.0 * a * c;
        if disc >= 0.0 {
            let sqrt_disc = disc.sqrt();
            consider((-b + sqrt_disc) / (2.0 * a));
            consider((-b - sqrt_disc) / (2.0 * a));
        }
    }
    (lo, hi)
}

#[cfg(test)]
mod tests {
    use super::*;
    use epiphany_layout_ir::Point;

    #[test]
    fn straight_line_extent_is_its_endpoints() {
        let cmds = vec![
            PathCommand::MoveTo(Point::new(0.0, 0.0)),
            PathCommand::LineTo(Point::new(2.0, 3.0)),
        ];
        assert_eq!(outline_extent(&cmds), Some([0.0, 0.0, 2.0, 3.0]));
    }

    #[test]
    fn empty_commands_have_no_extent() {
        assert_eq!(outline_extent(&[]), None);
    }

    #[test]
    fn curve_extrema_are_found_beyond_the_endpoints() {
        // The classic "hump" cubic (P0=(0,0), P1=(0,1), P2=(1,1), P3=(1,0)):
        // its peak y at t=0.5 is 0.75, well past either endpoint's y=0 — a
        // control-point-only (or endpoint-only) bound would miss it.
        let cmds = vec![
            PathCommand::MoveTo(Point::new(0.0, 0.0)),
            PathCommand::CurveTo {
                control1: Point::new(0.0, 1.0),
                control2: Point::new(1.0, 1.0),
                to: Point::new(1.0, 0.0),
            },
        ];
        let ext = outline_extent(&cmds).unwrap();
        assert!(
            (ext[3] - 0.75).abs() < 1e-5,
            "expected top ~0.75, got {}",
            ext[3]
        );
        assert_eq!(ext[0], 0.0, "left stays at the endpoints' x");
        assert_eq!(ext[2], 1.0, "right stays at the endpoints' x");
        assert_eq!(
            ext[1], 0.0,
            "bottom is the (only) minimum, at both endpoints"
        );
    }
}
