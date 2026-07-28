//! Round 0 accessibility-route probe, candidate C3 (iced).
//!
//! **ROUND-0 RESULT: FAIL. This probe does NOT satisfy round 0, and is kept
//! only as the evidence of why.** Adjudicated 2026-07-28 by coordinator
//! review; see `../DECISIONS.md`.
//!
//! It drives `accesskit_unix::Adapter` directly, which takes no window handle
//! -- only handlers -- and registers with AT-SPI from process identity. The
//! tree below is hand-built here: one window node, one button node. It is NOT
//! derived from iced's widget tree, does not track iced or real focus changes
//! (it sets one static focus at startup and never revises it), and routes no
//! actions.
//! The button in `view()` carries the same label purely by construction, which
//! is what made the transcript read as if iced had produced it.
//!
//! **Deleting iced from this probe would produce the identical readback.**
//! That is the disqualifying fact: round 0 asks whether the *candidate*
//! exposes an accessibility route, and a process-level side channel answers a
//! different question. A probe that proves nothing about its subject is a
//! probe-design defect, and it is recorded as one.
//!
//! The candidate limitation underneath it is separate and is what actually
//! fails the round: iced 0.14 ships no accessibility integration (`accesskit`
//! appears in no iced crate manifest), and its **stock runner** hands
//! application code neither the winit `ActiveEventLoop` nor a pre-visibility
//! `winit::window::Window`, both of which every `accesskit_winit::Adapter`
//! constructor requires. Scope that to the stock runner: `iced_winit`'s own
//! docs note a `conversion` module "for users that decide to implement a
//! custom event loop", so a hand-built shell remains **conceivable but
//! unproven** -- and it would mean owning the shell. Upstream iced #552 is
//! still open.

use accesskit::{Action, ActionHandler, ActionRequest, ActivationHandler, DeactivationHandler};
use accesskit::{Node as AccessNode, NodeId as AccessNodeId, Role, Tree, TreeId, TreeUpdate};
use accesskit_unix::Adapter as UnixAdapter;

use iced::widget::{button, column, text};
use iced::Element;

const WINDOW_NODE_ID: AccessNodeId = AccessNodeId(0);
const BUTTON_NODE_ID: AccessNodeId = AccessNodeId(1);
const BUTTON_NAME: &str = "EpiphanyProbeButton";
const WINDOW_TITLE: &str = "EpiphanyProbeIced";

fn build_tree() -> TreeUpdate {
    let mut root = AccessNode::new(Role::Window);
    root.set_children(vec![BUTTON_NODE_ID]);
    root.set_label(WINDOW_TITLE);

    let mut button_node = AccessNode::new(Role::Button);
    button_node.set_label(BUTTON_NAME);
    button_node.add_action(Action::Focus);

    TreeUpdate {
        nodes: vec![(WINDOW_NODE_ID, root), (BUTTON_NODE_ID, button_node)],
        tree: Some(Tree::new(WINDOW_NODE_ID)),
        tree_id: TreeId::ROOT,
        focus: BUTTON_NODE_ID,
    }
}

/// Returns the full static tree synchronously, so no event-loop plumbing
/// is needed to answer AT-SPI's initial tree request.
struct StaticActivationHandler;
impl ActivationHandler for StaticActivationHandler {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        Some(build_tree())
    }
}

/// Round 0 does not exercise actions (that is round 3's job); accept and
/// discard.
struct NoopActionHandler;
impl ActionHandler for NoopActionHandler {
    fn do_action(&mut self, _request: ActionRequest) {}
}

struct NoopDeactivationHandler;
impl DeactivationHandler for NoopDeactivationHandler {
    fn deactivate_accessibility(&mut self) {}
}

struct ProbeState {
    // Held for its lifetime, not read again: dropping it would tear down
    // the AT-SPI registration.
    _adapter: UnixAdapter,
}

impl Default for ProbeState {
    fn default() -> Self {
        let mut adapter = UnixAdapter::new(
            StaticActivationHandler,
            NoopActionHandler,
            NoopDeactivationHandler,
        );
        // Force the tree to materialize now rather than waiting for an
        // AT-SPI client's first request, and mark it focused so a reading
        // client sees a live, focused application rather than an inert one.
        adapter.update_if_active(build_tree);
        adapter.update_window_focus_state(true);
        Self { _adapter: adapter }
    }
}

#[derive(Debug, Clone, Copy)]
enum Message {
    Noop,
}

fn update(_state: &mut ProbeState, _message: Message) {}

fn view(_state: &ProbeState) -> Element<'_, Message> {
    column![
        text(WINDOW_TITLE),
        button(BUTTON_NAME).on_press(Message::Noop),
    ]
    .padding(20)
    .into()
}

fn main() -> iced::Result {
    iced::run(update, view)
}
