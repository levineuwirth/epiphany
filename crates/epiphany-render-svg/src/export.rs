//! Exporting a layout page by page: each page's primitives and the frame it is
//! drawn in, for the SVG and PDF renderers. The paper is the engraver's page
//! rectangle; a page whose ink runs past its paper (a system taller than the
//! page) is drawn in a frame extended to hold that ink, and the overrun is
//! reported, so an export never cuts music off.

use epiphany_layout_ir::ResolvedLayoutIR;

use crate::page::page_layout;
use crate::svg::{ink_frame, Frame};

/// One page of an export.
#[derive(Clone, PartialEq, Debug)]
pub struct ExportPage {
    /// The primitives the page's systems own.
    pub layout: ResolvedLayoutIR,
    /// The page's paper: where the engraver stacked it.
    pub paper: Frame,
    /// The frame it is drawn in: its paper, extended to hold any ink beyond it.
    pub frame: Frame,
}

impl ExportPage {
    /// How far, in staff spaces, the page's ink runs past its paper on the
    /// side it runs furthest; zero when it stays on the paper.
    pub fn overrun(&self) -> f32 {
        let (p, f) = (self.paper, self.frame);
        [
            p.left - f.left,
            p.bottom - f.bottom,
            (f.left + f.width) - (p.left + p.width),
            (f.bottom + f.height) - (p.bottom + p.height),
        ]
        .into_iter()
        .fold(0.0, f32::max)
    }
}

/// Every page of `layout`, each with its paper (`paper(index)`, the page's
/// rectangle in the layout's world, from 0) and the frame it is drawn in.
pub fn export_pages(layout: &ResolvedLayoutIR, paper: impl Fn(usize) -> Frame) -> Vec<ExportPage> {
    (0..layout.pages.len())
        .map(|index| {
            let page = page_layout(layout, index + 1).expect("the page exists");
            let paper = paper(index);
            let frame = match ink_frame(&page) {
                Some(ink) => union(paper, ink),
                None => paper,
            };
            ExportPage {
                layout: page,
                paper,
                frame,
            }
        })
        .collect()
}

/// The smallest frame holding both.
fn union(a: Frame, b: Frame) -> Frame {
    let left = a.left.min(b.left);
    let bottom = a.bottom.min(b.bottom);
    let right = (a.left + a.width).max(b.left + b.width);
    let top = (a.bottom + a.height).max(b.bottom + b.height);
    if left == a.left
        && bottom == a.bottom
        && right == a.left + a.width
        && top == a.bottom + a.height
    {
        return a;
    }
    Frame {
        left,
        bottom,
        width: right - left,
        height: top - bottom,
    }
}
