//! Token-efficient text rendering of snapshots for LLM contexts.
//! One line per interactive element; ~15-25 tokens per element.

use vakbrowse_core::Snapshot;

pub fn snapshot_text(s: &Snapshot) -> String {
    let mut out = String::with_capacity(64 + s.elements.len() * 48);
    out.push_str(&format!("Page: {}\nURL: {}\n", s.title, s.url));
    if s.elements.is_empty() {
        out.push_str("(no interactive elements)");
        return out;
    }
    let mut headings = s.headings.iter().peekable();
    for (i, e) in s.elements.iter().enumerate() {
        while let Some(h) = headings.next_if(|h| h.before <= i) {
            out.push_str(&"#".repeat(h.level as usize));
            out.push(' ');
            out.push_str(&compact(&h.text, 80));
            out.push('\n');
        }
        out.push_str(&e.r#ref.0);
        out.push('\t');
        out.push_str(&e.role);
        out.push_str(" \"");
        out.push_str(&compact(&e.name, 60));
        out.push('"');
        if !e.state.is_empty() {
            out.push_str(" [");
            out.push_str(&e.state.join(","));
            out.push(']');
        }
        if let Some(v) = &e.value {
            out.push_str(" value=\"");
            out.push_str(&compact(v, 80));
            out.push('"');
        }
        out.push('\n');
    }
    for h in headings {
        out.push_str(&format!(
            "{} {}\n",
            "#".repeat(h.level as usize),
            compact(&h.text, 80)
        ));
    }
    if s.omitted > 0 {
        out.push_str(&format!(
            "(+{} more interactive elements omitted; scroll or use find_element)\n",
            s.omitted
        ));
    }
    // Trailing newline is noise for tokens.
    while out.ends_with('\n') {
        out.pop();
    }
    out
}

fn compact(s: &str, max: usize) -> String {
    let one_line = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() <= max {
        return one_line;
    }
    let cut: String = one_line.chars().take(max).collect();
    format!("{cut}…")
}

/// One-screen capacity report (`status` on every text surface).
pub fn status_text(st: &crate::governor::ResourceStatus) -> String {
    let on = |b: bool| if b { "on" } else { "off" };
    let live = st
        .available_mb
        .map_or("unknown".to_string(), |m| format!("{m} MB"));
    format!(
        "host: {} MB ({}), {} CPUs\n\
         browser memory: {}/{} MB used, live free {live} (reserve {} MB)\n\
         costs: {} MB per session, {} MB per extra tab\n\
         sessions: {} ({} hibernated)\n\
         queue: {}/{} waiting · page work {}/{} · launches {}/{}\n\
         lean default: {} · private-network guard: {}",
        st.capacity.memory_mb,
        st.capacity.memory_source,
        st.capacity.cpus,
        st.used_mb,
        st.budget_mb,
        st.reserve_mb,
        st.session_cost_mb,
        st.tab_cost_mb,
        st.sessions,
        st.hibernated,
        st.queued,
        st.max_queue,
        st.active,
        st.max_active,
        st.launching,
        st.max_launches,
        on(st.lean_default),
        on(st.private_network_guard),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use vakbrowse_core::{ElementRef, SnapshotNode};

    #[test]
    fn renders_compact_lines() {
        let s = Snapshot {
            url: "https://x/".into(),
            title: "T".into(),
            elements: vec![
                SnapshotNode {
                    r#ref: ElementRef::new("@e1"),
                    role: "button".into(),
                    name: "Send application".into(),
                    value: None,
                    clickable: true,
                    state: vec![],
                },
                SnapshotNode {
                    r#ref: ElementRef::new("@e2"),
                    role: "textbox".into(),
                    name: "Search".into(),
                    value: Some("hello world".into()),
                    clickable: false,
                    state: vec!["required".into()],
                },
            ],
            headings: vec![vakbrowse_core::Heading {
                before: 1,
                level: 2,
                text: "Search".into(),
            }],
            omitted: 3,
        };
        let text = snapshot_text(&s);
        assert!(text.starts_with("Page: T\n"));
        assert!(text.contains("@e1\tbutton \"Send application\""));
        assert!(
            text.contains("## Search\n@e2\ttextbox \"Search\" [required] value=\"hello world\"")
        );
        assert!(text.contains("+3 more interactive elements omitted"));
        assert!(!text.ends_with('\n'));
    }

    #[test]
    fn empty_snapshot_note() {
        let s = Snapshot {
            url: "u".into(),
            title: "t".into(),
            elements: vec![],
            headings: vec![],
            omitted: 0,
        };
        assert!(snapshot_text(&s).contains("no interactive elements"));
    }
}
