//! Experimental **non-Chromium** engine backend: a tiny headless DOM.
//!
//! This is NOT a real rendering/JS engine — it exists to prove the
//! `EngineLauncher`/`PageOps` seam is genuinely swappable. It parses HTML
//! into an in-memory tree and implements the static operations (navigate,
//! snapshot, fill, extract, cookies) with **no JavaScript**: any op that
//! needs JS or a viewport returns a honest `VakError::Unsupported`.
//!
//! Honest limits (called out so agents don't trust this backend for flows that
//! need real page behavior):
//! - Form submit via JS event handlers: NOT supported (fills the field but
//!   never fires handlers). Use the CDP backend for that.
//! - `eval_text` / `wait_for_truthy` / `wait_for_url` by JS: unsupported
//!   (wait_for_url polls `location.href` with the timeout, CDP parity — but
//!   only the in-memory URL, so it cannot observe an external navigation that
//!   the DOM backend never received).
//! - `screenshot` / `click_at` / WebMCP / multi-tab: unsupported.
//!
//! Use it to drive the seam: `SessionManager::new(policy, Arc::new(DomLauncher))`.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use ureq::ResponseExt;

use vakbrowse_core::{
    Cookie, CookieInput, ElementRef, Extracted, Result, Snapshot, SnapshotNode, TabId, TabInfo,
    VakError, WebMcpTool,
};
use vakbrowse_engine::{
    ClickResult, EngineLauncher, LaunchOptions, Navigated, PageOps, validate_url,
};

mod js;
use js::JsRuntime;
pub(crate) mod selector;

/// --- Minimal tolerant HTML tokenizer + DOM tree ---

#[derive(Debug, Clone)]
struct Node {
    // tag name, lowercased. "" for a text node.
    tag: String,
    attrs: Vec<(String, String)>,
    text: String,
    children: Vec<Node>,
}

impl Node {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find_map(|(k, v)| {
            if k.eq_ignore_ascii_case(name) {
                Some(v.as_str())
            } else {
                None
            }
        })
    }
    fn set_attr(&mut self, name: &str, value: &str) {
        if let Some((_, v)) = self
            .attrs
            .iter_mut()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
        {
            *v = value.to_string();
        } else {
            self.attrs.push((name.to_lowercase(), value.to_string()));
        }
    }
}

const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];
const RAW_TEXT: &[&str] = &["script", "style"];
const RCDATA: &[&str] = &["textarea", "title"];

fn is_void(tag: &str) -> bool {
    VOID.contains(&tag.to_ascii_lowercase().as_str())
}

fn parse_attr(p: &str) -> Option<(String, String)> {
    // split a single `name=value` / `name="v"` / `name='v'` / `name` token.
    let (n, v) = match p.find('=') {
        Some(eq) => {
            let n = &p[..eq];
            let mut v = &p[eq + 1..];
            // Strip only ONE matching outer quote pair. A naive trim_matches
            // greedily eats inner quotes, truncating e.g.
            // `href="javascript:locate('a')"` -> missing close quote.
            if v.len() >= 2 {
                let (first, last) = (v.as_bytes()[0], v.as_bytes()[v.len() - 1]);
                if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
                    v = &v[1..v.len() - 1];
                }
            }
            (n, v)
        }
        None => (p, ""),
    };
    let n = n.trim();
    if n.is_empty() {
        return None;
    }
    // attribute values are HTML-entity-encoded
    Some((n.to_string(), unescape(v)))
}

/// Decode character references (`&amp;`/`&lt;`/`&gt;`/`&quot;`/`&apos;`) and
/// numeric refs (`&#8482;` / `&#x2122;`). Unknown refs are emitted literally.
/// Adequate for attribute values and text in this minimal backend.
fn unescape(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'&' {
            if let Some((ch, next)) = decode_entity(bytes, i) {
                out.push(ch);
                i = next;
                continue;
            } else {
                out.push('&');
                i += 1;
                continue;
            }
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// Try to decode an entity beginning at the `&` at `start`. Returns the decoded
/// char and the index just past the entity (consuming a trailing `;` when present).
fn decode_entity(b: &[u8], start: usize) -> Option<(char, usize)> {
    let mut j = start + 1; // past '&'
    if b.get(j) == Some(&b'#') {
        // numeric reference
        j += 1;
        let hex = matches!(b.get(j), Some(&b'x') | Some(&b'X'));
        if hex {
            j += 1;
        }
        let radix = if hex { 16 } else { 10 };
        let num_start = j;
        while j < b.len() {
            let d = b[j];
            let ok = d.is_ascii_digit() || (hex && d.is_ascii_hexdigit() && !d.is_ascii_digit());
            if !ok {
                break;
            }
            j += 1;
        }
        if j == num_start {
            return None;
        }
        let digits = std::str::from_utf8(&b[num_start..j]).ok()?;
        let val = u32::from_str_radix(digits, radix).ok()?;
        if b.get(j) == Some(&b';') {
            j += 1;
        }
        return char::from_u32(val).map(|c| (c, j));
    }
    // named reference
    let name_start = j;
    while j < b.len() && b[j] != b';' && b[j].is_ascii_alphanumeric() {
        j += 1;
    }
    let name = std::str::from_utf8(&b[name_start..j]).ok()?;
    let semi = b.get(j) == Some(&b';');
    if semi {
        j += 1;
    }
    let ch = match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        _ => return None,
    };
    Some((ch, j))
}

/// Index just past the next `>` starting at `start`, honoring `"`/`'` quoted
/// attribute values so a `>` inside a quoted value does not close the tag.
fn tag_end(html: &[u8], start: usize) -> usize {
    let mut i = start;
    let mut quote: Option<u8> = None;
    while i < html.len() {
        let c = html[i];
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                }
            }
            None => match c {
                b'"' | b'\'' => quote = Some(c),
                b'>' => return i + 1,
                _ => {}
            },
        }
        i += 1;
    }
    html.len()
}

/// Find the byte index of the `<` that begins a closing `</name` (case-
/// insensitive) at or after `from`. Returns `None` if not found.
fn find_close_tag(html: &[u8], name: &str, from: usize) -> Option<usize> {
    let needle = format!("</{}", name);
    let nb = needle.as_bytes();
    let mut i = from;
    while i + nb.len() <= html.len() {
        if html[i] == b'<' && html[i..i + nb.len()].eq_ignore_ascii_case(nb) {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn tokenize(html: &str) -> Vec<Token> {
    let bytes = html.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'<' => {
                if bytes.get(i + 1) == Some(&b'/') {
                    // closing tag
                    let end = tag_end(bytes, i);
                    let inner = html[i + 2..end.saturating_sub(1)].trim().to_lowercase();
                    out.push(Token::End(inner));
                    i = end;
                } else if bytes.get(i + 1) == Some(&b'?') || bytes.get(i + 1) == Some(&b'!') {
                    // comment / doctype / CDATA: skip to '>'
                    i = tag_end(bytes, i);
                } else {
                    // opening tag
                    let end = tag_end(bytes, i);
                    let inner = &html[i + 1..end.saturating_sub(1)];
                    let trimmed = inner.trim_end_matches('/').trim_end();
                    let tag = trimmed
                        .split_whitespace()
                        .next()
                        .unwrap_or("")
                        .to_lowercase();
                    let mut attrs = Vec::new();
                    for p in trimmed.split_whitespace().skip(1) {
                        if let Some((n, v)) = parse_attr(p) {
                            attrs.push((n, v));
                        }
                    }
                    let self_closing = inner.trim_end().ends_with('/');
                    out.push(Token::Start(tag.clone(), attrs, self_closing));
                    i = end;

                    // Raw-text elements: capture `<script>` body verbatim as a
                    // text child so it can be executed on load (a `>` or `<`
                    // inside must not be tokenized as markup); `<style>` body
                    // is dropped (not JS-runnable, not a11y-relevant).
                    if (tag == "script" || RAW_TEXT.iter().any(|r| *r == tag)) && !self_closing {
                        if let Some(start) = find_close_tag(bytes, &tag, i) {
                            let raw = html[i..start].trim().to_string();
                            if !raw.is_empty() {
                                out.push(Token::Text(raw));
                            }
                            out.push(Token::End(tag.clone()));
                            i = tag_end(bytes, start);
                        } else {
                            let raw = html[i..].trim().to_string();
                            if !raw.is_empty() {
                                out.push(Token::Text(raw));
                            }
                            out.push(Token::End(tag.clone()));
                            i = bytes.len();
                        }
                    } else if RCDATA.iter().any(|r| *r == tag) && !self_closing {
                        if let Some(start) = find_close_tag(bytes, &tag, i) {
                            let text = unescape(html[i..start].trim());
                            if !text.is_empty() {
                                out.push(Token::Text(text));
                            }
                            out.push(Token::End(tag.clone()));
                            i = tag_end(bytes, start);
                        } else {
                            let text = unescape(html[i..].trim());
                            if !text.is_empty() {
                                out.push(Token::Text(text));
                            }
                            out.push(Token::End(tag.clone()));
                            i = bytes.len();
                        }
                    }
                }
            }
            _ => {
                let start = i;
                while i < bytes.len() && bytes[i] != b'<' {
                    i += 1;
                }
                let t = unescape(html[start..i].trim());
                if !t.is_empty() {
                    out.push(Token::Text(t));
                }
            }
        }
    }
    out
}

#[derive(Debug)]
enum Token {
    Text(String),
    Start(String, Vec<(String, String)>, bool),
    End(String),
}

fn build_tree(tokens: &[Token]) -> Node {
    let mut it = tokens.iter().peekable();
    let mut children = Vec::new();
    build_children(&mut it, &mut children);
    Node {
        tag: "".into(),
        attrs: vec![],
        text: String::new(),
        children,
    }
}

/// Recursively consume `it`, appending child nodes to `out` until a matching
/// `End(name)` (or end of input) closes the current element.
fn build_children<'a>(
    it: &mut std::iter::Peekable<std::slice::Iter<'a, Token>>,
    out: &mut Vec<Node>,
) {
    while let Some(t) = it.peek() {
        match t {
            Token::Text(s) => {
                out.push(Node {
                    tag: String::new(),
                    attrs: vec![],
                    text: s.clone(),
                    children: vec![],
                });
                it.next();
            }
            Token::Start(tag, attrs, self_closing) => {
                if is_void(tag) || *self_closing {
                    out.push(Node {
                        tag: tag.clone(),
                        attrs: attrs.clone(),
                        text: String::new(),
                        children: vec![],
                    });
                    it.next();
                } else {
                    it.next(); // consume the Start token
                    let mut node = Node {
                        tag: tag.clone(),
                        attrs: attrs.clone(),
                        text: String::new(),
                        children: vec![],
                    };
                    build_children(it, &mut node.children);
                    // consume the matching End(name); for well-formed input
                    // (our only target here) it always matches the open tag.
                    let end_matches = matches!(it.peek(), Some(Token::End(n)) if n.eq_ignore_ascii_case(&node.tag));
                    if end_matches {
                        it.next();
                    }
                    out.push(node);
                }
            }
            Token::End(_) => return, // closes the parent element's children
        }
    }
}

// --- accessibility role inference for interactive elements ---

fn element_role(n: &Node) -> Option<String> {
    let tag = n.tag.to_ascii_lowercase();
    if tag.is_empty() {
        return None;
    }
    // honor explicit aria-role
    if let Some(r) = n.attr("role") {
        return Some(r.to_string());
    }
    let type_attr = n.attr("type").map(|t| t.to_ascii_lowercase());
    Some(
        match tag.as_str() {
            "a" if n.attr("href").is_some() => "link",
            "button" => "button",
            "select" => "combobox",
            "textarea" => "textbox",
            "option" => "option",
            "input" => match type_attr.as_deref() {
                None
                | Some("text" | "email" | "password" | "search" | "tel" | "url" | "number") => {
                    "textbox"
                }
                Some("checkbox") => "checkbox",
                Some("radio") => "radio",
                Some("submit" | "reset" | "button" | "image") => "button",
                _ => "textbox",
            },
            _ => return None,
        }
        .to_string(),
    )
}

/// Text content of a node's descendants (no tags).
fn text_content(n: &Node) -> String {
    let mut out = String::new();
    tc(n, &mut out);
    out
}
fn tc(n: &Node, out: &mut String) {
    if !n.text.is_empty() {
        out.push_str(&n.text);
    }
    for c in &n.children {
        tc(c, out);
    }
}

/// Resolve a relative or absolute URL against the document's base.
fn resolve_href(base: &str, href: &str) -> String {
    if href.starts_with("javascript:") || href.starts_with("data:") {
        return href.to_string();
    }
    if let Ok(base_url) = url::Url::parse(base)
        && let Ok(joined) = base_url.join(href)
    {
        return joined.to_string();
    }
    href.to_string()
}

/// Name for an interactive element: aria-label > placeholder > title >
/// associated <label> text > text content > (empty).
fn accessible_name(n: &Node) -> String {
    if let Some(v) = n.attr("aria-label") {
        return v.to_string();
    }
    if let Some(v) = n.attr("placeholder") {
        return v.to_string();
    }
    if let Some(v) = n.attr("title") {
        return v.to_string();
    }
    // label wrapping/associating (forms fixtures use wrapping <label>)
    // walk up — but our tree is flat; best-effort: text content of the node.
    let txt = text_content(n).trim().to_string();
    if !txt.is_empty() && n.tag == "button" {
        return txt;
    }
    txt
}

/// --- DomPage: a single in-memory document ---
pub struct DomPage {
    url: String,
    /// Shared DOM tree: read under `RwLock` by `DomPage` (snapshot/extract/fill)
    /// AND by the QuickJS worker's `document` callbacks, so JS mutations are
    /// visible to the Rust-side ops. `Arc` so a fresh worker (one per `load`)
    /// can capture a clone.
    doc: Arc<RwLock<Node>>,
    cookies: std::sync::Mutex<Vec<Cookie>>,
    /// Real-JS bridge (QuickJS on a dedicated thread). Recreated on every `load`
    /// so scripts run against a fresh context (correct nav semantics). `None`
    /// only if the worker thread failed to spawn; then JS-backed ops return
    /// `Unsupported`.
    js: Option<JsRuntime>,
    raw_source: String,
}

static TAB_SEQ: AtomicU64 = AtomicU64::new(1);

/// Parse `selector` and walk `doc`'s interactive elements (numbered `@eN`
/// in snapshot order), returning the stable refs matching the selector. Used by
/// both `DomPage::find_by_css` and the QuickJS worker's `_vkb_find` callback.
pub(crate) fn find_by_css(doc: &Node, selector: &str) -> Result<Vec<ElementRef>> {
    let sel = crate::selector::Selector::parse(selector)?;
    let mut out = Vec::new();
    let mut seen = 0usize;
    find_by_css_in_doc(doc, &sel, &mut Vec::new(), &mut seen, &mut out);
    Ok(out)
}

/// CSS selector search over interactive elements, numbered `@e{N}` in the same
/// pre-order the snapshot uses so returned refs are immediately clickable.
fn find_by_css_in_doc<'a>(
    n: &'a Node,
    sel: &crate::selector::Selector,
    ancestors: &mut Vec<&'a Node>,
    seen: &mut usize,
    out: &mut Vec<ElementRef>,
) {
    let is_interactive = element_role(n).is_some();
    if is_interactive {
        // `@e{N}` where N is 1-based among interactive elements in pre-order,
        // matching the numbering `snapshot` assigns.
        let refstr = format!("@e{}", *seen + 1);
        *seen += 1;
        if sel.matches(n, ancestors) {
            out.push(ElementRef(refstr));
        }
    }
    ancestors.push(n);
    for c in &n.children {
        find_by_css_in_doc(c, sel, ancestors, seen, out);
    }
    ancestors.pop();
}

fn interactive(doc: &Node) -> Vec<(&Node, String)> {
    let mut out = Vec::new();
    let mut label: Vec<String> = Vec::new();
    walk_labeled(doc, &mut label, &mut out);
    out
}

/// Depth-first walk carrying the text of each ancestor `<label>` so that
/// `<label>Name <input/></label>` yields "Name" as the input's name.
fn walk_labeled<'a>(n: &'a Node, label: &mut Vec<String>, out: &mut Vec<(&'a Node, String)>) {
    let pushed = n.tag == "label";
    if pushed {
        let full = text_content(n).trim().to_string();
        if !full.is_empty() {
            label.push(full);
        }
    }
    if element_role(n).is_some() {
        let lbl = label.last().cloned().unwrap_or_default();
        out.push((n, lbl));
    }
    for c in &n.children {
        walk_labeled(c, label, out);
    }
    if pushed && let Some(s) = label.pop() {
        let _ = s;
    }
}

impl DomPage {
    fn new() -> Self {
        let doc = Arc::new(RwLock::new(Node {
            tag: "".into(),
            attrs: vec![],
            text: String::new(),
            children: vec![],
        }));
        Self {
            url: "about:blank".into(),
            js: JsRuntime::new(Arc::clone(&doc)).ok(),
            doc,
            cookies: std::sync::Mutex::new(vec![]),
            raw_source: String::new(),
        }
    }

    async fn load(&mut self, url: &str, html: Option<&str>) -> Result<Navigated> {
        let mut next_url = url.to_string();
        let mut next_html = html.map(String::from);
        let mut depth = 0u8;
        loop {
            let parsed = validate_url(&next_url)?;
            let href = parsed.as_str().to_string();
            self.url = href.clone();
            if let Some(h) = next_html.take() {
                self.raw_source = h.clone();
                let built = build_tree(&tokenize(&h));
                self.doc = Arc::new(RwLock::new(built));
            } else if parsed.scheme() == "file" {
                let p = parsed
                    .to_file_path()
                    .map_err(|e| VakError::Engine(format!("bad file url: {e:?}")))?;
                let h = std::fs::read_to_string(&p).map_err(VakError::Io)?;
                self.raw_source = h.clone();
                let built = build_tree(&tokenize(&h));
                self.doc = Arc::new(RwLock::new(built));
            } else if parsed.scheme() == "about" {
                self.raw_source = String::new();
                self.doc = Arc::new(RwLock::new(Node {
                    tag: "".into(),
                    attrs: vec![],
                    text: String::new(),
                    children: vec![],
                }));
            } else if parsed.scheme() == "http" || parsed.scheme() == "https" {
                let fetch_url = href.clone();
                let (final_url, body) = tokio::task::spawn_blocking(move || -> Result<(String, String)> {
                    let agent = ureq::Agent::new_with_config(
                        ureq::Agent::config_builder()
                            .timeout_global(Some(std::time::Duration::from_secs(15)))
                            .build(),
                    );
                    let resp = agent
                        .get(&fetch_url)
                        .header(
                            "User-Agent",
                            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 \
                             (KHTML, like Gecko) Chrome/133.0.0.0 Safari/537.36",
                        )
                        .header(
                            "Accept",
                            "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
                        )
                        .call()
                        .map_err(|e| VakError::Engine(format!("network fetch failed: {e}")))?;
                    let final_uri = resp.get_uri().to_string();
                    let mut reader = resp.into_body().into_reader();
                    let mut text = String::new();
                    std::io::Read::read_to_string(&mut reader, &mut text).map_err(VakError::Io)?;
                    Ok((final_uri, text))
                })
                .await
                .map_err(|e| VakError::Engine(format!("fetch task join failed: {e}")))??;
                self.url = final_url;
                self.raw_source = body.clone();
                let built = build_tree(&tokenize(&body));
                self.doc = Arc::new(RwLock::new(built));
            } else {
                return Err(VakError::Engine(format!(
                    "dom backend: unsupported scheme '{}'",
                    parsed.scheme()
                )));
            }
            // Fresh JS worker for this document (isolates the context to this
            // page: scripts re-run, globals don't leak across navigations). The
            // old worker drops and its read-loop exits on `rx.recv()` error.
            self.js = JsRuntime::new(Arc::clone(&self.doc)).ok();
            // Reflect URL so `location.href` reads correctly; run inline scripts
            // (pure JS, no DOM API). A script may redirect via `location.href =
            // '<url>'` — honor it, depth-bounded to break redirect loops.
            if let Some(js) = &self.js {
                js.set_location(&href).await?;
                for script in self.collect_scripts() {
                    let _ = js.eval_text(&script).await;
                }
                let nav = js.check_nav().await?;
                if !nav.is_empty() && depth < 8 {
                    let target = resolve_href(&href, &nav);
                    if target.starts_with("file://")
                        || target.starts_with("about:")
                        || target.starts_with("http://")
                        || target.starts_with("https://")
                    {
                        next_url = target;
                        depth += 1;
                        continue;
                    }
                }
            }
            break;
        }
        let title = self.title_of();
        Ok(Navigated {
            url: self.url.clone(),
            title,
        })
    }

    /// Gather inline `<script>` text from the parsed tree (external `src`
    /// scripts are unsupported — no network fetch).
    fn collect_scripts(&self) -> Vec<String> {
        fn walk(n: &Node, out: &mut Vec<String>) {
            if n.tag == "script" && n.attr("src").is_none() {
                let t = text_content(n);
                if !t.is_empty() {
                    out.push(t);
                }
            }
            for c in &n.children {
                walk(c, out);
            }
        }
        let mut out = Vec::new();
        let guard = self.doc.read().unwrap_or_else(|e| e.into_inner());
        walk(&guard, &mut out);
        out
    }

    fn title_of(&self) -> String {
        fn find(n: &Node, depth: usize) -> String {
            if n.tag == "title" {
                return text_content(n);
            }
            if depth < 6 {
                for c in &n.children {
                    let t = find(c, depth + 1);
                    if !t.is_empty() {
                        return t;
                    }
                }
            }
            String::new()
        }
        let guard = self.doc.read().unwrap_or_else(|e| e.into_inner());
        find(&guard, 0)
    }
}

impl DomPage {
    /// Serialize a `Node` subtree to HTML string (for `source()`).
    fn serialize_html(&self, node: &Node) -> String {
        fn escape(s: &str) -> String {
            s.replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
                .replace('"', "&quot;")
                .replace('\'', "&#39;")
        }
        fn attr_str(n: &Node) -> String {
            if n.attrs.is_empty() {
                String::new()
            } else {
                let inner = n
                    .attrs
                    .iter()
                    .map(|(k, v)| format!("{k}=\"{}\"", escape(v)))
                    .collect::<Vec<_>>()
                    .join(" ");
                format!(" {inner}")
            }
        }
        fn ser(n: &Node, out: &mut String) {
            // Document-fragment node (empty tag, has children): recurse without
            // emitting a wrapper tag.
            if n.tag.is_empty() && !n.children.is_empty() {
                for child in &n.children {
                    ser(child, out);
                }
                return;
            }
            // Text node (empty tag, no children): emit escaped text.
            if n.tag.is_empty() {
                out.push_str(&escape(&n.text));
                return;
            }
            let tag = n.tag.to_lowercase();
            if RAW_TEXT.contains(&tag.as_str()) {
                let children = n.children.iter().map(|c| c.text.clone()).collect::<String>();
                out.push_str(&format!("<{tag}{}>", attr_str(n)));
                out.push_str(&children);
                out.push_str(&format!("</{tag}>"));
                return;
            }
            out.push_str(&format!("<{tag}{}>", attr_str(n)));
            if !is_void(&tag) {
                let was_raw = RCDATA.contains(&tag.as_str());
                if was_raw {
                    if let Some(t) = n.children.first() {
                        out.push_str(&escape(&t.text));
                    }
                } else {
                    for child in &n.children {
                        ser(child, out);
                    }
                }
                out.push_str(&format!("</{tag}>"));
            }
        }
        let mut out = String::new();
        ser(node, &mut out);
        out
    }
}

#[async_trait::async_trait]
impl PageOps for DomPage {
    async fn navigate(&mut self, url: &str) -> Result<Navigated> {
        self.load(url, None).await
    }
    async fn title(&self) -> Result<String> {
        Ok(self.title_of())
    }
    async fn extract(&mut self) -> Result<Extracted> {
        let mut txt = String::new();
        fn walk(n: &Node, out: &mut String) {
            if n.tag == "h1" || n.tag == "h2" || n.tag == "h3" {
                let t = text_content(n).trim().to_string();
                if !t.is_empty() {
                    out.push_str(&format!("{t}\n"));
                }
            }
            if n.tag == "p" {
                let t = text_content(n).trim().to_string();
                if !t.is_empty() {
                    out.push_str(&format!("\n{t}\n"));
                }
            }
            if n.tag == "li" {
                let t = text_content(n).trim().to_string();
                if !t.is_empty() {
                    out.push_str(&format!("- {t}\n"));
                }
            }
            for c in &n.children {
                walk(c, out);
            }
        }
        let guard = self.doc.read().unwrap_or_else(|e| e.into_inner());
        walk(&guard, &mut txt);
        if txt.is_empty() {
            txt = text_content(&guard);
        }
        Ok(Extracted {
            title: self.title_of(),
            url: self.url.clone(),
            text: txt,
            truncated: false,
        })
    }
    async fn back(&mut self) -> Result<Navigated> {
        Err(VakError::Unsupported(
            "dom backend: no history stack".into(),
        ))
    }
    async fn forward(&mut self) -> Result<Navigated> {
        Err(VakError::Unsupported(
            "dom backend: no history stack".into(),
        ))
    }
    async fn reload(&mut self) -> Result<Navigated> {
        // Re-parse the current document (re-read file if applicable).
        let url = self.url.clone();
        self.load(&url, None).await
    }
    async fn snapshot(&mut self) -> Result<Snapshot> {
        let guard = self.doc.read().unwrap_or_else(|e| e.into_inner());
        let items = interactive(&guard);
        let mut elements = Vec::with_capacity(items.len());
        for (n, lbl) in items {
            let refstr = format!("@e{}", elements.len() + 1);
            let role = element_role(n).unwrap_or_else(|| "generic".to_string());
            let mut value = None;
            if n.tag == "input" || n.tag == "textarea" {
                if let Some(v) = n.attr("value") {
                    value = Some(v.to_string());
                }
                if value.is_none() && !n.text.is_empty() {
                    value = Some(n.text.clone());
                }
            } else if n.tag == "select" {
                // first selected option's text
                for c in &n.children {
                    if c.tag == "option" && c.attr("selected").is_some() {
                        value = Some(text_content(c).trim().to_string());
                        break;
                    }
                }
            } else if n.tag == "option" {
                value = Some(text_content(n).trim().to_string());
            }
            let mut name = accessible_name(n);
            if name.is_empty() && !lbl.is_empty() {
                name = lbl;
            }
            let clickable = n.attr("href").is_some()
                || n.tag == "button"
                || n.tag == "select"
                || n.tag == "option";
            elements.push(SnapshotNode {
                r#ref: ElementRef(refstr),
                role,
                name,
                value,
                clickable,
            });
        }
        Ok(Snapshot {
            url: self.url.clone(),
            title: self.title_of(),
            elements,
        })
    }
    async fn eval_text(&self, expression: &str) -> Result<String> {
        match &self.js {
            Some(js) => js.eval_text(expression).await,
            None => Err(VakError::Unsupported(
                "dom backend: js runtime unavailable".into(),
            )),
        }
    }
    async fn click(&mut self, r: &ElementRef) -> Result<ClickResult> {
        let target = ref_to_index(&r.0);
        let (href, onclick) = target
            .map(|t| {
                let guard = self.doc.read().unwrap_or_else(|e| e.into_inner());
                let mut seen = 0usize;
                if let Some(n) = find_by_index_ref(&guard, t, &mut seen) {
                    (
                        n.attr("href").map(String::from),
                        n.attr("onclick").map(String::from),
                    )
                } else {
                    (None, None)
                }
            })
            .unwrap_or((None, None));

        if let Some(href) = href {
            // `javascript:` URLs: run the script on the JS engine; a non-empty
            // string result is treated as a navigation target (browser parity),
            // `void(...)`/non-string results stay (browser parity).
            if let Some(script) = href.strip_prefix("javascript:") {
                let outcome = match &self.js {
                    Some(js) => js.eval_href(script.trim()).await?,
                    None => "stay".to_string(),
                };
                if let Some(url) = outcome.strip_prefix("nav:") {
                    let target = resolve_href(&self.url, url);
                    if target.starts_with("file://")
                        || target.starts_with("about:")
                        || target.starts_with("http://")
                        || target.starts_with("https://")
                    {
                        let nav = self.load(&target, None).await?;
                        return Ok(ClickResult::navigated(nav.url));
                    }
                }
                return Ok(ClickResult::stayed());
            }

            let target = resolve_href(&self.url, &href);

            // In-page hash/fragment navigation (e.g. #cite_note-15, #History)
            let is_same_page_fragment = if let (Ok(u1), Ok(u2)) =
                (url::Url::parse(&self.url), url::Url::parse(&target))
            {
                u1.scheme() == u2.scheme()
                    && u1.host_str() == u2.host_str()
                    && u1.port() == u2.port()
                    && u1.path() == u2.path()
                    && u1.query() == u2.query()
                    && u1.fragment() != u2.fragment()
            } else {
                false
            };

            if is_same_page_fragment {
                self.url = target.clone();
                if let Some(js) = &self.js {
                    let _ = js.set_location(&target).await;
                }
                return Ok(ClickResult::navigated(target));
            }

            if target.starts_with("file://")
                || target.starts_with("about:")
                || target.starts_with("http://")
                || target.starts_with("https://")
            {
                let nav = self.load(&target, None).await?;
                return Ok(ClickResult::navigated(nav.url));
            }
        }

        if let Some(script) = onclick
            && let Some(js) = &self.js
        {
            let _ = js.eval_text(&script).await;
            let nav = js.check_nav().await?;
            if !nav.is_empty() {
                let target = resolve_href(&self.url, &nav);
                if target.starts_with("file://")
                    || target.starts_with("about:")
                    || target.starts_with("http://")
                    || target.starts_with("https://")
                {
                    let nav_res = self.load(&target, None).await?;
                    return Ok(ClickResult::navigated(nav_res.url));
                }
            }
        }

        Ok(ClickResult::stayed())
    }
    async fn fill(&mut self, r: &ElementRef, text: &str) -> Result<()> {
        if let Some(t) = ref_to_index(&r.0) {
            let mut guard = self.doc.write().unwrap_or_else(|e| e.into_inner());
            let mut seen = 0usize;
            if let Some(n) = find_by_index(&mut guard, t, &mut seen)
                && matches!(n.tag.as_str(), "input" | "textarea")
            {
                n.set_attr("value", text);
                if n.tag == "textarea" {
                    n.text = text.to_string();
                }
            }
        }
        Ok(())
    }
    async fn select_option(&mut self, r: &ElementRef, value: &str) -> Result<bool> {
        if let Some(t) = ref_to_index(&r.0) {
            let mut guard = self.doc.write().unwrap_or_else(|e| e.into_inner());
            let mut seen = 0usize;
            if let Some(n) = find_by_index(&mut guard, t, &mut seen).filter(|n| n.tag == "select") {
                for c in &mut n.children {
                    if c.tag == "option" {
                        c.set_attr(
                            "selected",
                            if c.attr("value").unwrap_or("") == value {
                                "selected"
                            } else {
                                ""
                            },
                        );
                    }
                }
                return Ok(true);
            }
        }
        Ok(false)
    }
    async fn press_key(&mut self, _key: &str) -> Result<()> {
        Err(VakError::Unsupported(
            "dom backend: no key-event dispatch".into(),
        ))
    }
    async fn scroll(&mut self, _dx: f64, _dy: f64) -> Result<()> {
        Ok(())
    }
    async fn wait_for_truthy(&self, expression: &str, timeout_ms: u64) -> Result<()> {
        let Some(js) = &self.js else {
            return Err(VakError::Unsupported(
                "dom backend: js runtime unavailable".into(),
            ));
        };
        let probe = format!("Boolean(({}))", expression);
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
        loop {
            match js.eval_text(&probe).await.map(|s| s == "true") {
                Ok(true) => return Ok(()),
                Ok(false) => {}
                Err(e) => return Err(e),
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(VakError::Timeout(format!("wait_for_truthy: {expression}")));
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    }
    async fn wait_for_url(&self, pattern: &str, timeout_ms: u64) -> Result<()> {
        let deadline =
            tokio::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
        loop {
            if self.url.contains(pattern) {
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(VakError::Timeout(format!(
                    "wait_for_url: {pattern} (href={})",
                    self.url
                )));
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    }
    async fn cookies(&self) -> Result<Vec<Cookie>> {
        Ok(self.cookies.lock().unwrap().clone())
    }
    async fn set_cookie(&mut self, cookie: &CookieInput) -> Result<()> {
        self.cookies.lock().unwrap().push(Cookie {
            name: cookie.name.clone(),
            value: cookie.value.clone(),
            domain: cookie.domain.clone(),
            path: cookie.path.clone(),
            secure: cookie.secure,
            http_only: cookie.http_only,
            session: true,
            same_site: cookie.same_site.clone(),
        });
        Ok(())
    }
    async fn clear_cookies(&self) -> Result<()> {
        self.cookies.lock().unwrap().clear();
        Ok(())
    }
    async fn set_download_dir(&mut self, _dir: &Path) -> Result<()> {
        Ok(())
    }
    async fn source(&self) -> Result<String> {
        if !self.raw_source.is_empty() {
            Ok(self.raw_source.clone())
        } else {
            Ok(self.serialize_html(&self.doc.read().unwrap_or_else(|e| e.into_inner())))
        }
    }
    async fn screenshot(&self, _full_page: bool) -> Result<Vec<u8>> {
        Err(VakError::Unsupported(
            "dom backend: no rendering viewport".into(),
        ))
    }
    async fn click_at(&mut self, _x: f64, _y: f64) -> Result<()> {
        Err(VakError::Unsupported(
            "dom backend: no rendering viewport".into(),
        ))
    }
    async fn webmcp_tools(&self) -> Result<Vec<WebMcpTool>> {
        Ok(Vec::new())
    }
    async fn webmcp_invoke(&self, _name: &str, _arguments_json: &str) -> Result<String> {
        Err(VakError::Unsupported("dom backend: no WebMCP host".into()))
    }
    async fn tabs(&self) -> Result<Vec<TabInfo>> {
        Ok(vec![TabInfo {
            id: TabId(format!("t{}", TAB_SEQ.fetch_add(1, Ordering::SeqCst))),
            url: self.url.clone(),
        }])
    }
    async fn new_tab(&mut self, _url: Option<&str>) -> Result<TabInfo> {
        Err(VakError::Unsupported("dom backend: single document".into()))
    }
    async fn switch_tab(&mut self, _tab: &TabId) -> Result<()> {
        Ok(())
    }
    async fn close_tab(&mut self, _tab: &TabId) -> Result<bool> {
        Ok(false)
    }

    /// CSS selector → `@eN` refs, numbered to match `snapshot`'s interactive-
    /// element ordering (pre-order). Returns the CDP/FFI surface a stable handle
    /// it can `click` immediately. No-op for non-interactive anchors (use
    /// `eval_text` for raw DOM queries).
    async fn find_by_css(&mut self, selector: &str) -> Result<Vec<ElementRef>> {
        let guard = self.doc.read().unwrap_or_else(|e| e.into_inner());
        crate::find_by_css(&guard, selector)
    }
}

impl DomPage {}

fn find_by_index<'a>(n: &'a mut Node, target: usize, seen: &mut usize) -> Option<&'a mut Node> {
    if !n.tag.is_empty() && element_role(n).is_some() {
        if *seen == target {
            return Some(n);
        }
        *seen += 1;
    }
    for c in &mut n.children {
        if let Some(r) = find_by_index(c, target, seen) {
            return Some(r);
        }
    }
    None
}

/// Resolve an `@eN` ref to the 0-based interactive-element index.
/// Returns `None` for invalid refs including `@e0` (refs are 1-based; the old
/// `saturating_sub` silently mapped `@e0` to index 0 = the first element).
fn ref_to_index(refstr: &str) -> Option<usize> {
    let n: usize = refstr.trim_start_matches("@e").parse().ok()?;
    n.checked_sub(1)
}

/// First mutable descendant (in `snapshot`'s order) at a given `@eN` index.
fn find_by_index_ref<'a>(n: &'a Node, target: usize, seen: &mut usize) -> Option<&'a Node> {
    if !n.tag.is_empty() && element_role(n).is_some() {
        if *seen == target {
            return Some(n);
        }
        *seen += 1;
    }
    for c in &n.children {
        if let Some(r) = find_by_index_ref(c, target, seen) {
            return Some(r);
        }
    }
    None
}
/// First descendant (in document order) with `id == id`.
fn find_by_id<'a>(n: &'a Node, id: &str) -> Option<&'a Node> {
    if n.attr("id").is_some_and(|v| v == id) {
        return Some(n);
    }
    for c in &n.children {
        if let Some(f) = find_by_id(c, id) {
            return Some(f);
        }
    }
    None
}

/// Mutable variant for `document.getElementById(...).textContent = ...` style writes.
fn find_by_id_mut<'a>(n: &'a mut Node, id: &str) -> Option<&'a mut Node> {
    if n.attr("id").is_some_and(|v| v == id) {
        return Some(n);
    }
    for c in &mut n.children {
        if let Some(f) = find_by_id_mut(c, id) {
            return Some(f);
        }
    }
    None
}

/// Experimental non-Chromium launcher: parses HTML in-process, no browser
/// binary. Honest about what it can't do (returns `Unsupported` via `DomPage`).
pub struct DomLauncher;

#[async_trait::async_trait]
impl EngineLauncher for DomLauncher {
    fn name(&self) -> &'static str {
        "dom (experimental)"
    }
    async fn ensure_executable(&self) -> Result<std::path::PathBuf> {
        // No binary required — this backend is pure Rust.
        Ok(std::path::PathBuf::from("dom"))
    }
    async fn launch(&self, options: &LaunchOptions) -> Result<Box<dyn PageOps>> {
        // Proxy / executable / user-data-dir / stealth / window size / extra
        // args are all chromium concepts; this backend ignores them (no
        // process to configure). The server's `open()` navigates separately
        // via `LaunchOptions` URL — LaunchOptions has no `url` field, so an
        // empty (about:blank) page is returned here.
        let _ = options;
        Ok(Box::new(DomPage::new()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form_html() -> &'static str {
        std::include_str!("../../../tests/fixtures/form.html")
    }

    #[tokio::test]
    async fn snapshot_finds_form_interactive_elements() {
        let mut page = DomPage::new();
        page.load("file:///tmp/form.html", Some(form_html()))
            .await
            .unwrap();
        let snap = page.snapshot().await.unwrap();
        let roles: Vec<_> = snap
            .elements
            .iter()
            .map(|e| (e.role.as_str(), e.name.clone()))
            .collect();
        assert!(
            roles
                .iter()
                .any(|(r, n)| *r == "textbox" && n.contains("Name")),
            "{roles:?}"
        );
        assert!(
            roles
                .iter()
                .any(|(r, n)| *r == "button" && n.contains("Send")),
            "{roles:?}"
        );
    }

    #[tokio::test]
    async fn fill_mutates_value_visible_on_next_snapshot() {
        let mut page = DomPage::new();
        page.load("file:///tmp/form.html", Some(form_html()))
            .await
            .unwrap();
        let before = page.snapshot().await.unwrap();
        let name_ref = before
            .elements
            .iter()
            .find(|e| e.role == "textbox" && e.name.contains("Name"))
            .unwrap()
            .r#ref
            .0
            .clone();
        page.fill(&ElementRef(name_ref.clone()), "Linus")
            .await
            .unwrap();
        let after = page.snapshot().await.unwrap();
        let v = after
            .elements
            .iter()
            .find(|e| e.r#ref.0 == name_ref)
            .unwrap();
        assert_eq!(
            v.value.as_deref(),
            Some("Linus"),
            "fill must persist in the DOM snapshot"
        );
    }

    #[tokio::test]
    async fn wait_for_url_matches_current_href_only() {
        let mut page = DomPage::new();
        page.load("file:///tmp/form.html", Some(form_html()))
            .await
            .unwrap();
        assert!(page.wait_for_url("form.html", 1000).await.is_ok());
        // Non-matching pattern must time out (polls, doesn't return instantly
        // as the old single-check did).
        let err = page
            .wait_for_url("nope_not_here", 50)
            .await
            .unwrap_err();
        assert!(matches!(err, VakError::Timeout(_)), "{err}");
    }

    #[tokio::test]
    async fn ref_e0_is_rejected_not_first_element() {
        let mut page = DomPage::new();
        page.load("file:///tmp/form.html", Some(form_html()))
            .await
            .unwrap();
        let snap = page.snapshot().await.unwrap();
        let first = &snap.elements[0];
        // @e0 is not a valid ref (refs are 1-based); clicking it must NOT
        // silently target the first element.
        let r0 = ElementRef::new("@e0");
        let r1 = first.r#ref.clone();
        // ref_to_index("@e0") => None => click is a no-op (stayed).
        let out = page.click(&r0).await.unwrap();
        assert!(
            matches!(out, ClickResult { navigated: false, url: None }),
            "@e0 must not navigate, got {out:?}"
        );
        // And @e0 does not equal @e1 (the first element).
        assert_ne!(r0, r1, "@e0 must not alias the first real ref");
    }

    // --- Real-JS (QuickJS) backend tests: chrome-free execution ---

    #[tokio::test]
    async fn js_eval_text_executes_arithmetic() {
        let mut page = DomPage::new();
        page.load("file:///tmp/x.html", Some("<html><body></body></html>"))
            .await
            .unwrap();
        assert_eq!(page.eval_text("1 + 2").await.unwrap(), "3");
        assert_eq!(page.eval_text("2 * 21").await.unwrap(), "42");
        // non-string primitive stringifies (eval_text semantics: Number/Bool ->
        // its string form, matching the CDP backend).
        assert_eq!(page.eval_text("void 0").await.unwrap(), "undefined");
        assert_eq!(page.eval_text("'hi'").await.unwrap(), "hi");
        assert_eq!(page.eval_text("42").await.unwrap(), "42");
        assert_eq!(page.eval_text("true").await.unwrap(), "true");
        // object literal must be an expression (parenthesized); a bare `{a:1}`
        // is a block+label in statement position — JS, not a bug.
        let json = page.eval_text("({a: 1, b: 2})").await.unwrap();
        assert!(json.contains("\"a\":1"), "got {json}");
        assert!(json.contains("\"b\":2"), "got {json}");
    }

    #[tokio::test]
    async fn js_inline_script_persists_in_eval_context() {
        let script = "var vkb_hidden = 7 * 6;";
        let mut page = DomPage::new();
        page.load(
            "file:///tmp/s.html",
            Some(&format!(
                "<html><body><script>{script}</script></body></html>"
            )),
        )
        .await
        .unwrap();
        assert_eq!(page.eval_text("vkb_hidden").await.unwrap(), "42");
    }

    #[tokio::test]
    async fn js_location_href_reflects_current_url() {
        let mut page = DomPage::new();
        page.load("file:///tmp/x.html", Some("<html><body></body></html>"))
            .await
            .unwrap();
        let got = page.eval_text("location.href").await.unwrap();
        assert_eq!(got, "file:///tmp/x.html", "got {got}");
    }

    #[tokio::test]
    async fn js_inline_script_redirects_via_location_href() {
        // A script that assigns `location.href` drives navigation (SPA pattern),
        // chrome-free: the page should end up on form.html with its content.
        let redirect = "<script>location.href = 'form.html';</script>";
        let mut page = DomPage::new();
        let url = url::Url::from_file_path(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/links.html"),
        )
        .unwrap()
        .to_string();
        page.load(&url, Some(redirect)).await.unwrap();
        let landed = page.eval_text("location.href").await.unwrap();
        assert!(
            landed.ends_with("fixtures/form.html"),
            "redirected to {landed}"
        );
        let txt = page.extract().await.unwrap().text;
        assert!(
            txt.contains("Adopt a pet"),
            "should render form.html h1: {txt}"
        );
    }

    #[tokio::test]
    async fn js_wait_for_truthy_resolves_on_condition() {
        let mut page = DomPage::new();
        page.load("file:///tmp/w.html", Some("<html><body></body></html>"))
            .await
            .unwrap();
        assert!(page.wait_for_truthy("9 > 5", 500).await.is_ok());
        assert!(page.wait_for_truthy("0 > 5", 100).await.is_err());
    }

    #[tokio::test]
    async fn js_click_javascript_void_stays_and_href_navigates() {
        // Base URL on the real fixtures dir so resolve_href() to "form.html"
        // lands on an existing fixture file.
        let base = url::Url::from_file_path(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/links.html"),
        )
        .unwrap()
        .to_string();
        let html = r#"<html><body>
          <a href="javascript:void(0)">no-op link</a>
          <a href="javascript:location.href='form.html'">js goto form</a>
          <a href="form.html">plain goto form</a>
        </body></html>"#;
        let mut page = DomPage::new();
        page.load(&base, Some(html)).await.unwrap();
        let snap = page.snapshot().await.unwrap();

        // `javascript:void(0)` => real JS runs, returns undefined => no nav.
        let void_ref = snap
            .elements
            .iter()
            .find(|e| e.name == "no-op link")
            .expect("void anchor")
            .r#ref
            .clone();
        let stayed = page.click(&ElementRef(void_ref.0)).await.unwrap();
        assert!(
            matches!(
                stayed,
                ClickResult {
                    navigated: false,
                    url: None
                }
            ),
            "{stayed:?}"
        );

        // `javascript:location.href='form.html'` => JS assignment returns the
        // target string => real JS-driven navigation to the fixture. (Stop
        // here: a later click would use stale refs after the doc swap.)
        let nav_ref = snap
            .elements
            .iter()
            .find(|e| e.name == "js goto form")
            .expect("js nav anchor")
            .r#ref
            .clone();
        let out = page.click(&ElementRef(nav_ref.0)).await.unwrap();
        assert!(
            matches!(
                out,
                ClickResult {
                    navigated: true,
                    ..
                }
            ),
            "{out:?}"
        );
        let url = out.url.expect("navigated url");
        assert!(url.ends_with("fixtures/form.html"), "navigated to {url}");

        // Plain (non-JS) `href="form.html"`: the original click path, on a fresh
        // page so its snapshot ref is valid after the earlier navigation.
        let mut page2 = DomPage::new();
        page2.load(&base, Some(html)).await.unwrap();
        let snap2 = page2.snapshot().await.unwrap();
        let plain_ref = snap2
            .elements
            .iter()
            .find(|e| e.name == "plain goto form")
            .expect("plain anchor")
            .r#ref
            .clone();
        let plain = page2.click(&ElementRef(plain_ref.0)).await.unwrap();
        assert!(
            matches!(
                plain,
                ClickResult {
                    navigated: true,
                    ..
                }
            ),
            "{plain:?}"
        );
    }

    #[tokio::test]
    async fn js_document_get_element_by_id_reads_text_content() {
        let mut page = DomPage::new();
        page.load("file:///tmp/form.html", Some(form_html()))
            .await
            .unwrap();
        // `document.getElementById('go').textContent` reads the button label from
        // the shared DOM tree through the QuickJS callback.
        let got = page
            .eval_text("document.getElementById('go').textContent")
            .await
            .unwrap();
        assert_eq!(got, "Send application", "got {got}");
    }

    #[tokio::test]
    async fn js_document_value_write_visible_to_snapshot() {
        // JS writing `.value` mutates the shared Node tree, which the Rust
        // snapshot path reads back — proving JS<->Rust DOM coupling.
        let mut page = DomPage::new();
        page.load("file:///tmp/form.html", Some(form_html()))
            .await
            .unwrap();
        page.eval_text("document.getElementById('name').value = 'Linus';")
            .await
            .unwrap();
        let snap = page.snapshot().await.unwrap();
        let name_el = snap
            .elements
            .iter()
            .find(|e| e.role == "textbox" && e.name.contains("Name"))
            .expect("name textbox in snapshot");
        assert_eq!(name_el.value.as_deref(), Some("Linus"), "{name_el:?}");
    }

    #[tokio::test]
    async fn js_form_submit_event_dispatches_handler_to_out() {
        // form.html wires `document.getElementById('f').addEventListener('submit',
        // h)` where h reads each field's `.value` and writes out.textContent.
        // Driving requestSubmit() through the JS event system (chrome-free)
        // must run h and land on #out — proving addEventListener + Event +
        // dispatch + DOM read/write all cooperate on the dom backend.
        let mut page = DomPage::new();
        page.load("file:///tmp/form.html", Some(form_html()))
            .await
            .unwrap();
        // Seed the fields the handler reads, then submit.
        page.eval_text(
            "document.getElementById('name').value = 'Linus';\
             document.getElementById('email').value = 'linus@example.com';\
             document.getElementById('pet').value = 'dog';\
             document.getElementById('msg').value = 'purr';\
             document.getElementById('f').requestSubmit();",
        )
        .await
        .unwrap();
        let out = page
            .eval_text("document.getElementById('out').textContent")
            .await
            .unwrap();
        assert!(out.starts_with("name=Linus"), "handler output: {out:?}");
        assert!(out.contains("email=linus@example.com"), "{out:?}");
        assert!(out.contains("pet=dog"), "{out:?}");
        assert!(out.contains("msg=purr"), "{out:?}");
    }

    #[test]
    fn tokenizer_tolerates_unclosed_tags() {
        let root = build_tree(&tokenize("<html><body><p>hi</p><input></body>"));
        fn count(n: &Node) -> usize {
            1 + n.children.iter().map(count).sum::<usize>()
        }
        assert!(count(&root) > 3);
    }

    #[test]
    fn tokenizer_decodes_entities_in_attrs_and_text() {
        let toks = tokenize(r#"<a href="/x?a=1&amp;b=2&amp;c=3">Tom &amp; Jerry</a>"#);
        // Start tag attrs should be entity-decoded
        let (name, href) = match &toks[0] {
            Token::Start(t, attrs, _) => (
                t.clone(),
                attrs.iter().find(|(k, _)| *k == "href").unwrap().1.clone(),
            ),
            _ => panic!("expected start {toks:?}"),
        };
        assert_eq!(name, "a");
        assert_eq!(href, "/x?a=1&b=2&c=3");
        // Trailing text token entity-decoded
        let txt = toks
            .iter()
            .find_map(|t| match t {
                Token::Text(s) => Some(s.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(txt, "Tom & Jerry");
    }

    #[test]
    fn tokenizer_numeric_refs_decode() {
        let toks = tokenize("&#8482; &#x2122;");
        let txt = toks
            .iter()
            .find_map(|t| match t {
                Token::Text(s) => Some(s.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(txt, "\u{2122} \u{2122}");
    }

    #[test]
    fn tokenizer_keeps_script_content_as_raw_text() {
        // The `>` and `<` inside script must NOT be tokenized as tags.
        let toks = tokenize("<script>if (a<b && c>d) { x('</p>'); }</script><p>after</p>");
        assert!(
            toks.iter()
                .filter(|t| matches!(t, Token::Start(t,_,_) if *t == "script"))
                .count()
                == 1
        );
        // only ONE <p> start should appear (the one in real markup)
        assert!(
            toks.iter()
                .filter(|t| matches!(t, Token::Start(t,_,_) if *t == "p"))
                .count()
                == 1
        );
        assert!(
            toks.iter()
                .any(|t| matches!(t, Token::Text(s) if s == "after"))
        );
    }

    #[test]
    fn tokenizer_textarea_captures_inner_text() {
        let toks = tokenize("<textarea>line &amp; more</textarea>");
        let txt = toks
            .iter()
            .find_map(|t| match t {
                Token::Text(s) => Some(s.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(txt, "line & more");
    }

    #[test]
    fn tokenizer_quoted_gt_in_attr_does_not_close_tag() {
        let toks = tokenize(r#"<input value="a>b">"#);
        match &toks[0] {
            Token::Start(_, attrs, _) => {
                let v = attrs.iter().find(|(k, _)| *k == "value").unwrap().1.clone();
                assert_eq!(v, "a>b");
            }
            _ => panic!("expected input start"),
        }
    }
}
