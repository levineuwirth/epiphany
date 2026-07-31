//! `ReportPart::AccessibilityIntegrationWiring`, half one — "getting that
//! tree to the platform: adapter lifecycle, event-loop plumbing, window and
//! bridge setup" (the F3 fix's own definition of this row). `a11y_subprocess.rs`
//! is the other half (the subprocess orchestration of the verifier).
//!
//! This module owns the `eframe::App` impl, the window options, and the
//! `eframe::run_native` call — the windowed route check 5 requires ("a real
//! window on the AT-SPI bus", the contract's own words) that Round 1's
//! headless binary does not have. It contains **no** semantic node-building
//! logic of its own (that is `a11y_node.rs`, which this module calls into)
//! and **no** visual rendering (dropped from an earlier revision of this
//! packet — see `a11y_node.rs`'s doc comment for why).

use round2_textkit::types::SpikeResolvedText;

pub struct A11yApp {
    pub fixture_id: String,
    pub resolved: SpikeResolvedText,
}

impl eframe::App for A11yApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.label(format!("Round 2 check 5 — fixture {}", self.fixture_id));
        ui.separator();

        let rect = egui::Rect::from_min_size(ui.min_rect().min, egui::vec2(900.0, 140.0));
        crate::a11y_node::build_text_run_node(ui, &self.fixture_id, rect, &self.resolved.text);

        // Keep repainting: AT-SPI clients query live state, and there is no
        // other event source driving redraws in this minimal app.
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(200));
    }
}

/// Opens the window and runs the event loop until the process is killed
/// (`a11y_subprocess.rs` is the one that kills it, once `verify.py` has read
/// the tree). The `eframe::run_native` app-id string
/// (`"EpiphanyRound2C1"`) is **not** what AT-SPI names the application —
/// AT-SPI's own application name tracks the process/binary name (measured
/// against `round0-evidence/c1-egui-readback.txt`'s precedent and
/// re-confirmed for this packet's own binary name); `a11y_subprocess.rs`'s
/// `A11Y_APP_NAME` constant is what actually has to match.
pub fn run(fixture_id: String, resolved: SpikeResolvedText) -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([960.0, 260.0])
            .with_title(format!("EpiphanyRound2C1 {fixture_id}")),
        ..Default::default()
    };

    eframe::run_native(
        "EpiphanyRound2C1",
        options,
        Box::new(move |_cc| {
            Ok(Box::new(A11yApp {
                fixture_id,
                resolved,
            }))
        }),
    )
}
