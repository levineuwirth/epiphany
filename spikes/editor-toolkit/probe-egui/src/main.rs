//! Round 0 accessibility-route probe, candidate C1 (modern egui).
//!
//! Opens a minimal window containing exactly one interactive widget (a
//! button) with a distinctive accessible name. egui's first-party AccessKit
//! integration (the `accesskit` feature, on by default in eframe 0.35) is
//! relied upon to publish that widget into the platform accessibility tree.
//! No manual accesskit_winit wiring is used here — the whole point of this
//! probe is to test the first-party route.
//!
//! This binary does not exit on its own: it is meant to be launched, given
//! a moment to register with AT-SPI, queried by the `a11y-verifier` script,
//! and then killed by the harness.

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([320.0, 180.0])
            .with_title("EpiphanyProbeEgui"),
        ..Default::default()
    };

    eframe::run_native(
        "EpiphanyProbeEgui",
        options,
        Box::new(|_cc| Ok(Box::new(ProbeApp))),
    )
}

struct ProbeApp;

impl eframe::App for ProbeApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            // The one accessible node round 0 needs: a button with a
            // distinctive, greppable name. egui buttons get AT-SPI role
            // "push button" and their name from the label text.
            let _ = ui.button("EpiphanyProbeButton");
        });
    }
}
