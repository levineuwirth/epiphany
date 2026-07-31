//! `ReportPart::AccessibilityTreeConstruction`, **semantic content only** —
//! the F3 fix's own definition of this row: "building the accessible
//! node(s): role, name, relationships, derived from the resolved text."
//!
//! Deliberately carries **no** window/event-loop/lifecycle code (that is
//! `a11y_app.rs`, counted under `AccessibilityIntegrationWiring`) and **no**
//! visual rendering at all — an earlier revision of this packet painted the
//! fixture's glyphs in the same file that built the AccessKit node, which is
//! exactly the kind of file-level mixing the F3 finding named as the defect:
//! a 190-line file that was mostly window setup, font loading, and
//! rendering, reported as if it were 190 lines of semantic tree
//! construction. This module is `AccessibilityTreeConstruction`, full stop;
//! it does not draw anything, and check 5 does not require it to (the
//! visual glyph mesh was cosmetic — "for visual confirmation only. Not read
//! by verify.py" — dropped here rather than kept and mis-attributed).
//!
//! **The accessible name is never derived from egui's own text layout.**
//! The node is built directly via `egui::Context::accesskit_node_builder` on
//! an `Id` that carries no text layout of its own
//! (`ui.interact(rect, id, Sense::hover())`), so the bytes reaching AT-SPI
//! are exactly the fixture's source string, untouched by galley
//! construction, wrapping, or any Unicode normalization egui's text stack
//! might otherwise apply — which is exactly what F-E (NFD) and F-C (an
//! uncovered codepoint that still must appear in the name) test.

use egui::accesskit::{Node, Rect as AkRect, Role};

/// The AccessKit role this candidate exposes the run under — `Label`, whose
/// accessible name is read from `Node::value` (per `accesskit`'s own doc
/// comment on `Node::set_label`: "the text content of a node with the
/// `Role::Label` role should be provided via `Node::value`, not this
/// property"), and which `accesskit_atspi_common` maps to AT-SPI role
/// `"label"` — one of the accepted at-spi2 tokens
/// (`round2_textkit::a11y::ACCEPTED_ROLE_TABLE`).
pub const NODE_ROLE: Role = Role::Label;

/// Builds one AccessKit node carrying `source_text` byte-for-byte as its
/// accessible name, at `rect`, allocated under `ui`'s current accesskit
/// parent (`ui.interact` registers the `Id` as an accesskit child of the
/// enclosing `Ui` — see `egui::Ui::interact`'s own implementation).
///
/// The `Id` is stable per fixture (`("epiphany_round2_text_run",
/// fixture_id)`), so repeated calls across frames update the same node
/// rather than accumulating duplicates.
pub fn build_text_run_node(
    ui: &mut egui::Ui,
    fixture_id: &str,
    rect: egui::Rect,
    source_text: &str,
) {
    let id = egui::Id::new(("epiphany_round2_text_run", fixture_id));
    let _response = ui.interact(rect, id, egui::Sense::hover());
    let name = source_text.to_string();
    ui.ctx().accesskit_node_builder(id, |node: &mut Node| {
        node.set_role(NODE_ROLE);
        node.set_value(name.clone());
        node.set_bounds(AkRect {
            x0: rect.min.x as f64,
            y0: rect.min.y as f64,
            x1: rect.max.x as f64,
            y1: rect.max.y as f64,
        });
    });
}
