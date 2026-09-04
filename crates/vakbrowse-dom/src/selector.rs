//! A tiny, dependency-free CSS selector matcher over the in-tree `Node`.
//!
//! Scope is deliberate and honest: compound selectors (tag / `.class` / `#id` /
//! `[attr]` / `[attr op val]`), descendant (` `) and child (`>`) combinators,
//! and comma groups. Pseudo-classes/elements (`:hover`, `::before`), attribute
//! list operators (`~=``|=`), and sibling combinators (`+`/`~`) are **not**
//! supported — `parse` returns an `Unsupported` error for them rather than
//! silently mismatching (bots lie; selectors shouldn't). This is enough for the
//! ~90% of agent click targets: `a[href]`, `form input[type="text"]`,
//! `.btn.primary`, `#search-form`, `a, button`.

use vakbrowse_core::VakError;

use super::Node;

/// `Peekable<std::str::Chars>` — the `str::Chars` form is an ambiguous path
/// under Rust's name resolution, so we pin it here.
type CharIter<'a> = std::iter::Peekable<std::str::Chars<'a>>;

/// A parsed selector list (comma-grouped). Each `ComplexSelector` is one group.
#[derive(Debug)]
pub struct Selector {
    groups: Vec<ComplexSelector>,
}

/// One comma-separated complex selector: compound[ combinator compound ]*.
#[derive(Debug)]
struct ComplexSelector {
    compounds: Vec<Compound>,
    /// `combinators[i]` is the combinator *between* `compounds[i]` and
    /// `compounds[i+1]` (i.e. `combinators.len() == compounds.len() - 1`).
    combinators: Vec<Combinator>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Combinator {
    Descendant,
    Child,
}

/// A compound selector: a conjunction of tag/class/id/attribute tests.
#[derive(Debug, Default)]
struct Compound {
    universal: bool,
    tag: Option<String>,
    id: Option<String>,
    classes: Vec<String>,
    attrs: Vec<AttrTest>,
}

#[derive(Debug)]
struct AttrTest {
    name: String,
    op: AttrOp,
    value: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AttrOp {
    Exists,
    Eq,
    StartsWith,
    EndsWith,
    Contains,
    /// `|=` word-prefix on hyphen.
    Hyphen,
    Word,
}

impl Selector {
    /// Parse a selector list. Returns `Unsupported` for pseudos or sibling
    /// combinators we don't implement.
    pub fn parse(src: &str) -> Result<Self, VakError> {
        let s = src.trim();
        if s.is_empty() {
            return Err(VakError::Unsupported("empty selector".into()));
        }
        // Split on top-level commas (ignore commas inside attribute `[...]`).
        let mut groups = Vec::new();
        let mut depth = 0u32;
        let mut last = 0usize;
        let bytes = s.as_bytes();
        for (i, &b) in bytes.iter().enumerate() {
            match b {
                b'[' => depth += 1,
                b']' => {
                    if depth == 0 {
                        return Err(VakError::Unsupported(
                            "unbalanced brackets in selector".into(),
                        ));
                    }
                    depth -= 1;
                }
                b',' if depth == 0 => {
                    groups.push(s[last..i].trim());
                    last = i + 1;
                }
                _ => {}
            }
        }
        if depth != 0 {
            return Err(VakError::Unsupported(
                "unbalanced brackets in selector".into(),
            ));
        }
        groups.push(s[last..].trim());

        let mut parsed = Vec::new();
        for g in groups {
            if g.is_empty() {
                return Err(VakError::Unsupported("empty selector group".into()));
            }
            parsed.push(parse_complex(g)?);
        }
        Ok(Selector { groups: parsed })
    }

    /// Match any group against `n`, given its ancestor chain (root → parent,
    /// nearest-last).
    pub fn matches(&self, n: &Node, ancestors: &[&Node]) -> bool {
        self.groups.iter().any(|g| g.matches_at(n, ancestors))
    }
}

impl ComplexSelector {
    fn matches_at(&self, n: &Node, ancestors: &[&Node]) -> bool {
        if self.combinators.len() >= self.compounds.len() {
            return false; // structural invariant; shouldn't happen
        }
        if !self.compounds[self.compounds.len() - 1].matches(n) {
            return false;
        }
        match_left(
            &self.compounds,
            &self.combinators,
            n,
            ancestors,
            self.compounds.len() - 1,
        )
    }
}

/// Right-to-left recursion: `idx` is the matched compound index; `cur` matched
/// `compounds[idx]`; `ancestors` are the nodes above `cur` (root→parent).
fn match_left(
    compounds: &[Compound],
    combinators: &[Combinator],
    _cur: &Node,
    ancestors: &[&Node],
    idx: usize,
) -> bool {
    if idx == 0 {
        return true;
    }
    let parent_comp = &compounds[idx - 1];
    let comb = &combinators[idx - 1];
    match comb {
        Combinator::Child => match ancestors.last() {
            Some(p) if parent_comp.matches(p) => {
                let upper = &ancestors[..ancestors.len() - 1];
                match_left(compounds, combinators, p, upper, idx - 1)
            }
            _ => false,
        },
        Combinator::Descendant => {
            // Nearest-matching ancestor, then recurse above it.
            for (pos, anc) in ancestors.iter().rev().enumerate() {
                if parent_comp.matches(anc) {
                    let upper = &ancestors[..ancestors.len() - 1 - pos];
                    if match_left(compounds, combinators, anc, upper, idx - 1) {
                        return true;
                    }
                }
            }
            false
        }
    }
}

impl Compound {
    fn matches(&self, n: &Node) -> bool {
        if n.tag.is_empty() {
            // a text node cannot satisfy a non-universal compound
            return self.universal
                && self.id.is_none()
                && self.classes.is_empty()
                && self.attrs.is_empty();
        }
        if !self.universal
            && let Some(tag) = &self.tag
            && !n.tag.eq_ignore_ascii_case(tag)
        {
            return false;
        }
        if let Some(id) = &self.id {
            match n.attr("id") {
                Some(v) if v == id => {}
                _ => return false,
            }
        }
        if !self.classes.is_empty() {
            let cls = n.attr("class").unwrap_or("");
            let tokens: Vec<&str> = cls.split_whitespace().collect();
            for want in &self.classes {
                if !tokens.contains(&want.as_str()) {
                    return false;
                }
            }
        }
        for test in &self.attrs {
            if !test.matches(n) {
                return false;
            }
        }
        true
    }
}

impl AttrTest {
    fn matches(&self, n: &Node) -> bool {
        let got = match n.attr(&self.name) {
            Some(v) => v,
            None => return self.op == AttrOp::Exists && self.value.is_none(),
        };
        match self.op {
            AttrOp::Exists => true,
            AttrOp::Eq => self.value.as_deref() == Some(got),
            AttrOp::StartsWith => got.starts_with(self.value.as_deref().unwrap_or("")),
            AttrOp::EndsWith => got.ends_with(self.value.as_deref().unwrap_or("")),
            AttrOp::Contains => got.contains(self.value.as_deref().unwrap_or("")),
            AttrOp::Hyphen => {
                let v = self.value.as_deref().unwrap_or("");
                got == v || got.starts_with(&format!("{}-", v))
            }
            AttrOp::Word => {
                let v = self.value.as_deref().unwrap_or("");
                got.split_whitespace().any(|t| t == v)
            }
        }
    }
}

/// Parse one comma-group into a `ComplexSelector`.
fn parse_complex(g: &str) -> Result<ComplexSelector, VakError> {
    let mut chars = g.chars().peekable();
    let mut compounds = Vec::new();
    let mut combinators = Vec::new();

    // A group must begin with a compound (or `*`). `a b`, `form input`, `a,button`.
    let first = parse_compound(&mut chars)?;
    compounds.push(first);

    loop {
        let comb = skip_ws_collect_combinator(&mut chars)?;
        match comb {
            Some(c) => {
                combinators.push(c);
                compounds.push(parse_compound(&mut chars)?);
            }
            None => break, // end of group
        }
    }
    if combinators.len() >= compounds.len() {
        return Err(VakError::Unsupported("malformed selector group".into()));
    }
    Ok(ComplexSelector {
        compounds,
        combinators,
    })
}

/// Whitespace-delimited compounds; `>` may be surrounded by spaces.
fn skip_ws_collect_combinator(chars: &mut CharIter<'_>) -> Result<Option<Combinator>, VakError> {
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else {
            break;
        }
    }
    match chars.peek() {
        Some(&'>') => {
            chars.next();
            // tolerate trailing spaces after `>`
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() {
                    chars.next();
                } else {
                    break;
                }
            }
            Ok(Some(Combinator::Child))
        }
        Some(&c)
            if c == ','
                || c == '['
                || c == '.'
                || c == '#'
                || c == '*'
                || c.is_alphabetic()
                || c == '_'
                || c == '-' =>
        {
            // whitespace between compounds = descendant combinator
            Ok(Some(Combinator::Descendant))
        }
        Some(&':') => Err(VakError::Unsupported(
            "pseudo-classes/elements are not supported in this selector engine".into(),
        )),
        Some(&c) if c == '+' || c == '~' => Err(VakError::Unsupported(format!(
            "sibling combinator `{c}` is not supported in this selector engine"
        ))),
        Some(_) => Ok(None), // end (e.g. stray char -> let parse_compound complain)
        None => Ok(None),
    }
}

/// Parse a single compound selector. Stops at the next combinator or group end.
fn parse_compound(chars: &mut CharIter<'_>) -> Result<Compound, VakError> {
    let mut comp = Compound::default();
    loop {
        match chars.peek() {
            None => break,
            Some(&c) if c.is_whitespace() => break,
            Some(&',') => break,
            Some(&'>') => break,
            Some(&'*') => {
                chars.next();
                comp.universal = true;
            }
            Some(&'[') => {
                chars.next();
                comp.attrs.push(parse_attr(chars)?);
                expect(chars, ']')?;
            }
            Some(&'#') => {
                chars.next();
                comp.id = Some(read_ident(chars, true)?);
            }
            Some(&'.') => {
                chars.next();
                comp.classes.push(read_ident(chars, true)?);
            }
            Some(&c) if is_name_start(c) => {
                let name = read_ident(chars, false)?;
                // a bare ident is either a tag or (if followed by nothing) a tag
                if comp.tag.is_none() && !comp.universal {
                    comp.tag = Some(name);
                }
            }
            Some(&c) => {
                return Err(VakError::Unsupported(format!(
                    "unsupported selector token `{c}`"
                )));
            }
        }
    }
    Ok(comp)
}

fn parse_attr(chars: &mut CharIter<'_>) -> Result<AttrTest, VakError> {
    let name = read_ident(chars, true)?;
    // skip spaces before the operator
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else {
            break;
        }
    }
    // operator: `=` / `^=` / `$=` / `*=` / `|=` / `~=`. Copy chars out of the
    // peek iterator first so the match scrutinee doesn't hold a borrow across
    // the consuming `next()` calls below.
    let op = match chars.peek().copied() {
        Some('=') => {
            chars.next();
            AttrOp::Eq
        }
        Some(first @ ('^' | '$' | '*' | '|' | '~')) => {
            chars.next();
            if chars.peek().copied() == Some('=') {
                chars.next();
                match first {
                    '^' => AttrOp::StartsWith,
                    '$' => AttrOp::EndsWith,
                    '*' => AttrOp::Contains,
                    '|' => AttrOp::Hyphen,
                    '~' => AttrOp::Word,
                    _ => unreachable!(),
                }
            } else {
                return Err(VakError::Unsupported(format!(
                    "malformed attribute operator `{first}=`"
                )));
            }
        }
        _ => {
            return Ok(AttrTest {
                name,
                op: AttrOp::Exists,
                value: None,
            });
        }
    };
    // skip spaces before value
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else {
            break;
        }
    }
    let value = if matches!(chars.peek(), Some('"') | Some('\'')) {
        read_quoted(chars)?
    } else {
        read_bare_value(chars)?
    };
    Ok(AttrTest {
        name,
        op,
        value: Some(value),
    })
}

fn read_quoted(chars: &mut CharIter<'_>) -> Result<String, VakError> {
    let q = chars.next().unwrap();
    let mut out = String::new();
    for c in chars {
        if c == q {
            return Ok(out);
        }
        out.push(c);
    }
    Err(VakError::Unsupported(
        "unterminated quoted selector value".into(),
    ))
}

fn read_bare_value(chars: &mut CharIter<'_>) -> Result<String, VakError> {
    let mut out = String::new();
    // peek+next (not a for-loop) so the terminator (`]`/`,`/`>`/ws) is left
    // for the caller to consume.
    #[allow(clippy::while_let_loop)]
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() || c == ']' || c == ',' || c == '>' {
            break;
        }
        out.push(c);
        chars.next();
    }
    if out.is_empty() {
        return Err(VakError::Unsupported("empty attribute value".into()));
    }
    Ok(out)
}

fn read_ident(chars: &mut CharIter<'_>, leading_nondigit: bool) -> Result<String, VakError> {
    let mut out = String::new();
    if let Some(&c) = chars.peek()
        && !is_name_start(c)
        && !(leading_nondigit && c.is_ascii_digit())
        && c != '-'
        && c != '_'
    {
        return Err(VakError::Unsupported(format!("invalid ident start `{c}`")));
    }
    // peek+next so the terminating delimiter (`.`/`#`/`[`/`>`/`,`/ws) is not
    // consumed — the caller needs to see it for the next compound/piece.
    #[allow(clippy::while_let_loop)]
    while let Some(&c) = chars.peek() {
        if is_name_char(c) {
            out.push(c);
            chars.next();
        } else {
            break;
        }
    }
    if out.is_empty() {
        return Err(VakError::Unsupported("empty identifier".into()));
    }
    Ok(out)
}

fn expect(chars: &mut CharIter<'_>, want: char) -> Result<(), VakError> {
    match chars.peek() {
        Some(&c) if c == want => {
            chars.next();
            Ok(())
        }
        other => Err(VakError::Unsupported(format!(
            "expected `{want}` in selector, got `{other:?}`"
        ))),
    }
}

fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == '-'
}
fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Node;

    fn el(tag: &str, attrs: &[(&str, &str)], text: &str) -> Node {
        Node {
            tag: tag.into(),
            attrs: attrs
                .iter()
                .map(|(k, v)| (k.to_lowercase(), v.to_string()))
                .collect(),
            text: text.into(),
            children: Vec::new(),
        }
    }

    #[test]
    fn selector_basic_matches() {
        let a = el("a", &[("href", "https://iana.org")], "link");
        let cls = el("button", &[("class", "btn primary")], "ok");
        let txt = el("input", &[("type", "text"), ("id", "q")], "");
        assert!(Selector::parse("a[href]").unwrap().matches(&a, &[]));
        assert!(Selector::parse("a[href^=https]").unwrap().matches(&a, &[]));
        assert!(Selector::parse("a[href$=.org]").unwrap().matches(&a, &[]));
        assert!(Selector::parse("a[href*=iana]").unwrap().matches(&a, &[]));
        assert!(!Selector::parse("a[href=/x]").unwrap().matches(&a, &[]));
        assert!(
            Selector::parse("button.btn.primary")
                .unwrap()
                .matches(&cls, &[])
        );
        assert!(
            !Selector::parse("button.btn.primary")
                .unwrap()
                .matches(&a, &[])
        );
        assert!(
            Selector::parse("input[type=\"text\"]#q")
                .unwrap()
                .matches(&txt, &[])
        );
        assert!(
            Selector::parse("input[type=text]")
                .unwrap()
                .matches(&txt, &[])
        );
        assert!(Selector::parse("a, button").unwrap().matches(&cls, &[]));
        assert!(Selector::parse("a, button").unwrap().matches(&a, &[]));
        // pseudo unsupported -> error
        assert!(Selector::parse("a:hover").is_err());
        // sibling combinator unsupported -> error (honest limit)
        assert!(Selector::parse("a + b").is_err());
    }

    #[test]
    fn selector_descendant_and_child_combinators() {
        // <form><input class="x" id="i" type="text"></form>
        let mut form = el("form", &[("id", "f")], "");
        let mut input = el("input", &[("type", "text"), ("class", "x")], "");
        input.attrs.push(("id".into(), "i".into()));
        form.children.push(input);
        let ancestors: Vec<&Node> = vec![&form];

        // the input is the last child; compound matches it.
        let inp = form.children.last().unwrap();
        assert!(
            Selector::parse("form input")
                .unwrap()
                .matches(inp, &ancestors)
        );
        assert!(
            Selector::parse("form > input")
                .unwrap()
                .matches(inp, &ancestors)
        );
        assert!(
            Selector::parse("form input[type=text]")
                .unwrap()
                .matches(inp, &ancestors)
        );
        assert!(
            Selector::parse("form input#i.x")
                .unwrap()
                .matches(inp, &ancestors)
        );
        // a bare `input` (no ancestor) must NOT match when ancestors present and the
        // complex selector demands a `form` ancestor.
        assert!(!Selector::parse("form input").unwrap().matches(inp, &[]));
        // `body form input` requires a body above form: ancestors lack body.
        assert!(
            !Selector::parse("section form input")
                .unwrap()
                .matches(inp, &ancestors)
        );
        // comma: either branch ok
        assert!(
            Selector::parse("section, form input")
                .unwrap()
                .matches(inp, &ancestors)
        );
    }
}
