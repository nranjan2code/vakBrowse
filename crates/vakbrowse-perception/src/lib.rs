//! Perception: turns a backend-neutral accessibility tree into a compact,
//! token-efficient `Snapshot` with stable element refs (`@eN`).
//!
//! The engine layer extracts raw AX data; this crate owns all policy about
//! *what agents see* and *how refs stay stable* across snapshots.

use std::collections::HashMap;
use vakbrowse_core::{ElementRef, Heading, Snapshot, SnapshotNode};

/// Flat AX node as produced by any backend. Tree order is reconstructed
/// from `child_ids` (reading order = child order).
#[derive(Debug, Clone)]
pub struct FlatAxNode {
    pub id: String,
    pub ignored: bool,
    pub role: Option<String>,
    pub name: Option<String>,
    pub value: Option<String>,
    pub child_ids: Vec<String>,
    /// Widget state tokens (see `SnapshotNode::state`).
    pub state: Vec<String>,
    /// Heading level for `role == "heading"`.
    pub level: Option<u8>,
}

/// Hard cap on interactive elements per snapshot. Keeps a link-farm page from
/// blowing an agent's context; the overflow is reported, not silently lost.
pub const MAX_SNAPSHOT_ELEMENTS: usize = 500;
const MAX_HEADINGS: usize = 40;
const MAX_HEADING_CHARS: usize = 80;

/// Roles an agent can act on. Everything else is prose, not a control.
const INTERACTIVE_ROLES: &[&str] = &[
    "button",
    "link",
    "textbox",
    "searchbox",
    "combobox",
    "listbox",
    "checkbox",
    "radio",
    "switch",
    "slider",
    "spinbutton",
    "option",
    "tab",
    "menuitem",
    "menuitemcheckbox",
    "menuitemradio",
    "treeitem",
];

/// Roles that are page chrome for humans but noise for agents.
const SKIPPED_ROLES: &[&str] = &[
    "banner",
    "contentinfo",
    "complementary",
    "none",
    "presentation",
];

/// Allocates and remembers element refs so the same AX node keeps its ref
/// across consecutive snapshots; refs are invalidated by navigation (callers
/// reset the book) or supersession.
#[derive(Debug, Default)]
pub struct RefBook {
    by_ax_id: HashMap<String, String>,
    next: usize,
}

impl RefBook {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reset all mappings (on navigation): refs restart at @e1.
    pub fn reset(&mut self) {
        self.by_ax_id.clear();
        self.next = 0;
    }

    fn assign(&mut self, ax_id: &str) -> String {
        if let Some(existing) = self.by_ax_id.get(ax_id) {
            return existing.clone();
        }
        self.next += 1;
        let r = format!("@e{}", self.next);
        self.by_ax_id.insert(ax_id.to_string(), r.clone());
        r
    }
}

/// What perception produced: the agent-facing snapshot plus the internal
/// mapping needed to resolve refs back to AX node ids.
#[derive(Debug, Clone)]
pub struct SnapshotBuild {
    pub snapshot: Snapshot,
    /// element ref -> AX node id
    pub ref_to_ax: HashMap<ElementRef, String>,
}

/// Reconstruct reading order from flat nodes (first root = node whose
/// parent is absent from the set), filter to interactive elements, assign
/// stable refs.
pub fn build_snapshot(
    url: &str,
    title: &str,
    flat: &[FlatAxNode],
    book: &mut RefBook,
) -> SnapshotBuild {
    let by_id: HashMap<&str, &FlatAxNode> = flat.iter().map(|n| (n.id.as_str(), n)).collect();

    let mut order: Vec<&FlatAxNode> = Vec::with_capacity(flat.len());
    let mut stack: Vec<String> = Vec::new();

    // Roots: nodes not referenced as anyone's child, in document order.
    let is_child: HashMap<&str, ()> = flat
        .iter()
        .flat_map(|n| n.child_ids.iter().map(|c| c.as_str()))
        .map(|c| (c, ()))
        .collect();
    for node in flat.iter().rev() {
        if !is_child.contains_key(node.id.as_str()) {
            stack.push(node.id.clone());
        }
    }

    while let Some(id) = stack.pop() {
        let Some(node) = by_id.get(id.as_str()) else {
            continue;
        };
        let node: &FlatAxNode = node;
        // Children pushed reversed so they pop in document order.
        for child in node.child_ids.iter().rev() {
            stack.push(child.clone());
        }
        if node.ignored {
            continue;
        }
        order.push(node);
    }

    let mut elements = Vec::new();
    let mut headings = Vec::new();
    let mut omitted = 0usize;
    let mut ref_to_ax = HashMap::new();

    for node in order {
        let Some(role) = node.role.as_deref() else {
            continue;
        };
        if role == "heading" {
            let text: String = node
                .name
                .as_deref()
                .unwrap_or("")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            if !text.is_empty() && headings.len() < MAX_HEADINGS {
                headings.push(Heading {
                    before: elements.len(),
                    level: node.level.unwrap_or(2).clamp(1, 6),
                    text: text.chars().take(MAX_HEADING_CHARS).collect(),
                });
            }
            continue;
        }
        if SKIPPED_ROLES.contains(&role) {
            continue;
        }
        let interactive = INTERACTIVE_ROLES.contains(&role);
        let named_container = role == "group" && node.name.is_some();
        if !interactive && !named_container {
            continue;
        }

        if elements.len() >= MAX_SNAPSHOT_ELEMENTS {
            omitted += 1;
            continue;
        }
        let r = book.assign(&node.id);
        ref_to_ax.insert(ElementRef(r.clone()), node.id.clone());
        elements.push(SnapshotNode {
            r#ref: ElementRef(r),
            role: role.to_string(),
            name: node.name.clone().unwrap_or_default(),
            value: node.value.clone().filter(|v| !v.is_empty()),
            clickable: matches!(
                role,
                "button"
                    | "link"
                    | "tab"
                    | "menuitem"
                    | "menuitemcheckbox"
                    | "menuitemradio"
                    | "checkbox"
                    | "radio"
                    | "switch"
                    | "option"
                    | "treeitem"
                    | "combobox"
            ),
            state: node.state.clone(),
        });
    }

    SnapshotBuild {
        snapshot: Snapshot {
            url: url.to_string(),
            title: title.to_string(),
            elements,
            headings,
            omitted,
        },
        ref_to_ax,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, role: &str, name: &str, children: &[&str]) -> FlatAxNode {
        FlatAxNode {
            id: id.to_string(),
            ignored: false,
            role: (!role.is_empty()).then(|| role.to_string()),
            name: (!name.is_empty()).then(|| name.to_string()),
            value: None,
            child_ids: children.iter().map(|c| c.to_string()).collect(),
            state: vec![],
            level: None,
        }
    }

    #[test]
    fn extracts_interactive_elements_in_reading_order() {
        let flat = vec![
            node("root", "WebArea", "Form", &["banner", "form"]),
            node("banner", "banner", "", &[]),
            node("form", "form", "", &["name", "submit", "link1"]),
            node("name", "textbox", "Your name", &[]),
            node("submit", "button", "Submit", &[]),
            node("link1", "link", "Help", &[]),
        ];
        let mut book = RefBook::new();
        let build = build_snapshot("u", "t", &flat, &mut book);

        let roles: Vec<&str> = build
            .snapshot
            .elements
            .iter()
            .map(|e| e.role.as_str())
            .collect();
        assert_eq!(roles, ["textbox", "button", "link"]);
        assert_eq!(build.snapshot.elements[0].r#ref.0, "@e1");
        assert_eq!(build.snapshot.elements[0].name, "Your name");
        assert!(build.snapshot.elements[1].clickable);
        assert!(!build.snapshot.elements[0].clickable);
    }

    #[test]
    fn refs_are_stable_and_new_nodes_append() {
        let mut book = RefBook::new();
        let first = vec![node("a", "button", "A", &[]), node("b", "link", "B", &[])];
        let b1 = build_snapshot("u", "t", &first, &mut book);
        assert_eq!(b1.snapshot.elements[0].r#ref.0, "@e1");
        assert_eq!(b1.snapshot.elements[1].r#ref.0, "@e2");

        // DOM mutated: same nodes plus one new.
        let second = vec![
            node("a", "button", "A", &[]),
            node("b", "link", "B", &[]),
            node("c", "textbox", "C", &[]),
        ];
        let b2 = build_snapshot("u", "t", &second, &mut book);
        assert_eq!(b2.snapshot.elements[0].r#ref.0, "@e1");
        assert_eq!(b2.snapshot.elements[1].r#ref.0, "@e2");
        assert_eq!(b2.snapshot.elements[2].r#ref.0, "@e3");

        // Navigation resets.
        book.reset();
        let b3 = build_snapshot("u", "t", &second, &mut book);
        assert_eq!(b3.snapshot.elements[0].r#ref.0, "@e1");
    }

    #[test]
    fn skips_ignored_and_noise() {
        let mut banner = node("x", "banner", "", &[]);
        banner.ignored = true;
        let flat = vec![
            node("root", "WebArea", "", &["x", "y"]),
            banner,
            node("y", "presentation", "", &[]),
            node("z", "button", "Go", &[]),
        ];
        let mut book = RefBook::new();
        let build = build_snapshot("u", "t", &flat, &mut book);
        assert_eq!(build.snapshot.elements.len(), 1);
        assert_eq!(build.snapshot.elements[0].role, "button");
    }

    #[test]
    fn state_headings_and_clickable_form_controls() {
        let mut cb = node("cb", "checkbox", "Subscribe", &[]);
        cb.state = vec!["checked".into(), "disabled".into()];
        let mut h = node("h", "heading", "  Shoes \n sale ", &[]);
        h.level = Some(2);
        let flat = vec![
            node("root", "WebArea", "", &["h", "b1", "cb"]),
            h,
            node("b1", "button", "Buy", &[]),
            cb,
        ];
        let mut book = RefBook::new();
        let snap = build_snapshot("u", "t", &flat, &mut book).snapshot;
        assert_eq!(snap.headings.len(), 1);
        assert_eq!(snap.headings[0].text, "Shoes sale");
        assert_eq!(snap.headings[0].before, 0, "heading precedes the button");
        assert_eq!(snap.elements[1].state, ["checked", "disabled"]);
        assert!(snap.elements[1].clickable, "checkboxes are clicked");
    }

    #[test]
    fn oversized_pages_are_capped_and_counted() {
        let n = MAX_SNAPSHOT_ELEMENTS + 25;
        let ids: Vec<String> = (0..n).map(|i| format!("l{i}")).collect();
        let mut flat = vec![node(
            "root",
            "WebArea",
            "",
            &ids.iter().map(String::as_str).collect::<Vec<_>>(),
        )];
        flat.extend(ids.iter().map(|i| node(i, "link", "x", &[])));
        let mut book = RefBook::new();
        let build = build_snapshot("u", "t", &flat, &mut book);
        assert_eq!(build.snapshot.elements.len(), MAX_SNAPSHOT_ELEMENTS);
        assert_eq!(build.snapshot.omitted, 25);
        assert_eq!(build.ref_to_ax.len(), MAX_SNAPSHOT_ELEMENTS);
    }

    #[test]
    fn values_pass_through() {
        let mut tb = node("v", "textbox", "Search", &[]);
        tb.value = Some("hello".into());
        let flat = vec![node("root", "WebArea", "", &["v"]), tb];
        let mut book = RefBook::new();
        let build = build_snapshot("u", "t", &flat, &mut book);
        assert_eq!(build.snapshot.elements[0].value.as_deref(), Some("hello"));
    }
}
