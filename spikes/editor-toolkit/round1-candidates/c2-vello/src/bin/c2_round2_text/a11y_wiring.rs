//! Check 5, `ReportPart::AccessibilityIntegrationWiring`: getting the tree
//! `a11y_tree.rs` builds onto the platform — **product-side only**: adapter
//! lifecycle, event loop, window and bridge setup, and tree publication.
//!
//! **Ruling (H1/J2): nothing that exists only to drive or await the
//! verifier lives here.** A real editor shipping this stack keeps exactly
//! what this file has — the window, the `accesskit_winit::Adapter`, the
//! event loop, publishing the tree `a11y_tree.rs` builds — and none of the
//! coordination that spawns `a11y-verifier/verify.py`, waits for it, or
//! decides what its output means, because that machinery exists only
//! because this spike scores itself out-of-process. That coordination is
//! `a11y_subprocess.rs`'s (`ReportPart::FixtureAndReportPlumbing`). An
//! earlier revision kept the worker thread, the readiness channel, and the
//! verifier's own result type in this file because the event loop "must run
//! while the subprocess does" — true, but that is a reason to expose a
//! narrow product-side surface the plumbing drives, not a reason to keep
//! the coordination itself here.
//!
//! **The seam, concretely.** [`run_window`] is generic over `T` and knows
//! nothing about verifiers, subprocesses, or `A11yRoundResult` — it runs the
//! window, calls `on_tree_published` exactly once (synchronously, from the
//! winit thread, the instant the tree has actually been pushed to the
//! platform), and blocks until *something* calls [`FinishHandle::finish`]
//! with a `T`, then closes the window and returns that `T`. What `T` is,
//! what `on_tree_published` does with the handle it receives (spawn a
//! thread; run a subprocess; anything), and how the result gets computed
//! are entirely the caller's concern (`a11y_subprocess::run_a11y_round`,
//! the only caller in this packet). This is deliberately reusable for
//! reasons that have nothing to do with check 5's verifier — the window
//! lifecycle a real product needs is exactly this and no more.
//!
//! vello ships no accessibility layer of its own, so — as `probe-vello`'s
//! Round 0 precedent already established for this candidate — this is a
//! **manual `accesskit_winit` wiring**: an accessibility tree built by hand
//! (`a11y_tree.rs`) and pushed through `accesskit_winit::Adapter`, driven
//! from a real winit `ApplicationHandler`. Unlike `probe-vello`, this mode
//! does not also drive a vello render pass: check 5 asks only whether the
//! run appears in the live platform tree as its source string, and the
//! headless rendering that answers check 1 already lives in `render.rs`.
//! Skipping the GPU surface here is a real simplification, named as one
//! rather than silently taken — see `c2_round2_text.rs`'s cost record.

use std::sync::Arc;

use accesskit_winit::{Adapter, Event as AccessKitEvent, WindowEvent as AccessKitWindowEvent};
use anyhow::{anyhow, Result};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::window::{Window, WindowId};

use crate::a11y_tree::{build_initial_tree, WINDOW_TITLE};

enum AppEvent<T> {
    AccessKit(AccessKitEvent),
    /// Delivered by [`FinishHandle::finish`] — the window closes and
    /// [`run_window`] returns this value. This is the entire coordination
    /// surface: what produces `T`, and when, is never this file's concern.
    Finish(T),
}

impl<T> From<AccessKitEvent> for AppEvent<T> {
    fn from(e: AccessKitEvent) -> Self {
        AppEvent::AccessKit(e)
    }
}

/// Handed to `on_tree_published` exactly once, the instant [`run_window`]'s
/// tree has actually been pushed to the platform. Calling
/// [`FinishHandle::finish`] (from any thread) is the only way the window
/// closes and `run_window` returns — the window otherwise waits
/// indefinitely, which is why every caller must eventually call it.
pub struct FinishHandle<T: Send + 'static> {
    proxy: EventLoopProxy<AppEvent<T>>,
}

impl<T: Send + 'static> FinishHandle<T> {
    pub fn finish(&self, value: T) {
        let _ = self.proxy.send_event(AppEvent::Finish(value));
    }
}

struct A11yApp<T: Send + 'static> {
    proxy: EventLoopProxy<AppEvent<T>>,
    fixture_texts: [String; 5],
    window: Option<Arc<Window>>,
    adapter: Option<Adapter>,
    on_tree_published: Option<Box<dyn FnOnce(FinishHandle<T>) + Send>>,
    result: Option<T>,
}

impl<T: Send + 'static> ApplicationHandler<AppEvent<T>> for A11yApp<T> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_inner_size(LogicalSize::new(480.0, 320.0))
            .with_title(WINDOW_TITLE);
        let window = Arc::new(
            event_loop
                .create_window(attrs)
                .expect("failed to create the round 2 a11y probe window"),
        );
        // Manual accesskit_winit wiring, exactly probe-vello's Round 0 route
        // (this file's module doc comment), minus the vello render pass.
        let adapter = Adapter::with_event_loop_proxy(event_loop, &window, self.proxy.clone());
        self.window = Some(window);
        self.adapter = Some(adapter);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(window) = self.window.clone() else {
            return;
        };
        if window.id() != window_id {
            return;
        }
        if let Some(adapter) = &mut self.adapter {
            adapter.process_event(&window, &event);
        }
        if let WindowEvent::CloseRequested = event {
            event_loop.exit();
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: AppEvent<T>) {
        match event {
            AppEvent::AccessKit(ak_event) => {
                if let AccessKitWindowEvent::InitialTreeRequested = ak_event.window_event {
                    let texts = self.fixture_texts.clone();
                    if let Some(adapter) = &mut self.adapter {
                        // Tree publication: push the tree a11y_tree.rs
                        // built, onto the platform, via the adapter this
                        // file owns the lifecycle of.
                        adapter.update_if_active(move || build_initial_tree(&texts));
                    }
                    // The tree is now actually live. Notify the caller
                    // exactly once, synchronously — this file has no
                    // opinion on what happens next, only that it *can*
                    // happen now. The callback must not block (it runs on
                    // the event loop's own thread); every real caller
                    // spawns a thread and returns immediately.
                    if let Some(cb) = self.on_tree_published.take() {
                        let handle = FinishHandle {
                            proxy: self.proxy.clone(),
                        };
                        cb(handle);
                    }
                }
            }
            AppEvent::Finish(value) => {
                self.result = Some(value);
                event_loop.exit();
            }
        }
    }
}

/// Opens the one probe window and builds its accessibility tree
/// (`a11y_tree::build_initial_tree`); calls `on_tree_published` exactly
/// once, the instant that tree is actually live, handing it a
/// [`FinishHandle`]; blocks until something calls
/// [`FinishHandle::finish`], then closes the window and returns the
/// delivered value.
///
/// This function is the entire product-side surface (H1/J2's ruling): it
/// knows nothing about verifiers, subprocesses, or check-5 scoring — `T` is
/// whatever the caller needs delivered, and `on_tree_published` is where the
/// caller's own coordination (spawning a thread, running a subprocess,
/// anything) begins. `a11y_subprocess::run_a11y_round` is the only caller
/// in this packet.
pub fn run_window<T: Send + 'static>(
    fixture_texts: [String; 5],
    on_tree_published: impl FnOnce(FinishHandle<T>) + Send + 'static,
) -> Result<T> {
    let event_loop = EventLoop::<AppEvent<T>>::with_user_event().build()?;
    let proxy = event_loop.create_proxy();

    let mut app = A11yApp {
        proxy,
        fixture_texts,
        window: None,
        adapter: None,
        on_tree_published: Some(Box::new(on_tree_published)),
        result: None,
    };
    event_loop.run_app(&mut app)?;

    app.result.take().ok_or_else(|| {
        anyhow!("a11y probe window closed with no value delivered via FinishHandle::finish")
    })
}
