//! Check 5, `ReportPart::AccessibilityTreeConstruction`: building the
//! accessible node(s) — role, name, relationships — derived from the
//! resolved text. **Semantic content only.** Getting this tree to the
//! platform (adapter lifecycle, event-loop plumbing, window/bridge setup,
//! subprocess orchestration of the verifier) is a *different* part of the
//! cost table and lives in `a11y_wiring.rs`, never here — the coordinator's
//! common attribution rule requires every `ReportPart` to map to a disjoint
//! set of whole files, and this file is exactly and only the semantic half.
//!
//! **One window, five sibling nodes.** winit does not support tearing an
//! `EventLoop` down and building a second one in the same process on every
//! platform, so rather than open and close five windows in sequence,
//! `build_initial_tree` builds one window's tree carrying one
//! `Role::Paragraph` child per fixture (F-A..F-E, name = that fixture's
//! exact source string) — `a11y_wiring.rs` scores all five fixtures against
//! that single live window, one `a11y-verifier/verify.py` subprocess
//! invocation per fixture.

use accesskit::{Node as AccessNode, NodeId as AccessNodeId, Role, Tree, TreeId, TreeUpdate};

pub const WINDOW_TITLE: &str = "EpiphanyC2Round2Text";
pub const ROOT_ID: AccessNodeId = AccessNodeId(0);
pub const FIXTURE_NODE_IDS: [AccessNodeId; 5] = [
    AccessNodeId(1),
    AccessNodeId(2),
    AccessNodeId(3),
    AccessNodeId(4),
    AccessNodeId(5),
];

/// One `Role::Paragraph` node whose accessible name is `text` **verbatim** —
/// the fixture's exact source string, never a shaped/ligated rendering of
/// it. `Role::Paragraph` maps to AT-SPI2 role `"paragraph"`, one of recipe
/// §8.2's accepted at-spi2 roles (verified against
/// `accesskit_atspi_common-0.19.1`'s `Role::Paragraph => AtspiRole::Paragraph`
/// mapping and `atspi-common-0.13.0`'s role-name table `"paragraph"`, both in
/// this workspace's lockfile).
///
/// **Deliberately not `Role::Label`**, despite it also being accepted:
/// `accesskit_consumer::Node::label_comes_from_value` special-cases exactly
/// `Role::Label` to read the accessible name from the node's *value*
/// property rather than its *label* property (`accesskit_consumer-0.36.0`
/// `node.rs:735`, in this workspace's lockfile). Measured directly on this
/// packet's first run: a `Role::Label` node with `set_label(text)` and no
/// `set_value` reached AT-SPI with an accessible name of `""` — precisely
/// the `name-empty` prohibited outcome recipe §8.3 pins, and not a
/// substitution or a drop, but a role/property mismatch this candidate's own
/// choice of role caused. `Role::Paragraph` carries no such special case, so
/// `set_label` alone is sufficient.
pub fn build_fixture_node(text: &str) -> AccessNode {
    let mut node = AccessNode::new(Role::Paragraph);
    node.set_label(text);
    node
}

pub fn build_root() -> AccessNode {
    let mut node = AccessNode::new(Role::Window);
    node.set_children(FIXTURE_NODE_IDS.to_vec());
    node.set_label(WINDOW_TITLE);
    node
}

pub fn build_initial_tree(fixture_texts: &[String; 5]) -> TreeUpdate {
    let mut nodes = vec![(ROOT_ID, build_root())];
    for (id, text) in FIXTURE_NODE_IDS.iter().zip(fixture_texts.iter()) {
        nodes.push((*id, build_fixture_node(text)));
    }
    TreeUpdate {
        nodes,
        tree: Some(Tree::new(ROOT_ID)),
        tree_id: TreeId::ROOT,
        focus: FIXTURE_NODE_IDS[0],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_root_carries_all_five_fixture_children_in_order() {
        let root = build_root();
        assert_eq!(root.role(), Role::Window);
        assert_eq!(root.children(), &FIXTURE_NODE_IDS[..]);
    }

    #[test]
    fn a_fixture_node_is_a_paragraph_carrying_the_exact_text_as_its_label() {
        let node = build_fixture_node("Coro \u{0627}");
        assert_eq!(node.role(), Role::Paragraph);
        assert_eq!(node.label(), Some("Coro \u{0627}"));
    }

    #[test]
    fn the_initial_tree_has_six_nodes_and_focuses_the_first_fixture() {
        let texts: [String; 5] = [
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
            "e".to_string(),
        ];
        let update = build_initial_tree(&texts);
        assert_eq!(update.nodes.len(), 6);
        assert_eq!(update.focus, FIXTURE_NODE_IDS[0]);
        assert_eq!(update.nodes[1].1.label(), Some("a"));
        assert_eq!(update.nodes[5].1.label(), Some("e"));
    }
}
