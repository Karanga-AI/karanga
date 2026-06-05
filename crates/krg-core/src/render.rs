//! Schema-driven projection (read side) and authoring parse (write side).
//!
//! Read: nodes → Karanga Markdown (interface §3, §8). Inline content is stored
//! *as* canonical Karanga Markdown (format §7), so rendering a text-bearing
//! node is direct; this module also owns the normative normalizer
//! ([`normalize_inline`]) that produces the canonical form on write, and the
//! plaintext projection ([`strip_inline`]).

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use crate::model::{Attrs, Node, Value};
use crate::Result;

/// Render a single node to Karanga Markdown.
pub fn render_node(node: &Node) -> String {
    match node.ty.as_str() {
        "heading" => {
            let level = attr_int(node, "level").unwrap_or(1).clamp(1, 6) as usize;
            format!("{} {}", "#".repeat(level), content_str(node))
        }
        "paragraph" => content_str(node).to_string(),
        "table-cell" => content_str(node).to_string(),
        "code" => {
            let lang = attr_str(node, "language").unwrap_or("");
            format!("```{lang}\n{}\n```", content_str(node))
        }
        "divider" => "---".to_string(),
        // Containers: own content is empty; children are rendered by section/
        // document rendering. Render a minimal shell.
        "blockquote" => "> ".to_string(),
        "media" => {
            let alt = attr_str(node, "alt").unwrap_or("");
            let src = attr_str(node, "src")
                .map(str::to_string)
                .or_else(|| attr_str(node, "asset").map(|a| format!("media/{a}")))
                .unwrap_or_default();
            format!("![{alt}]({src})")
        }
        t if t.contains(':') => format!(":::{t}\n:::"), // declared custom block type
        _ => content_str(node).to_string(), // list / list-item / table / table-row → empty own content
    }
}

fn content_str(node: &Node) -> &str {
    node.content.as_deref().unwrap_or("")
}

/// Render whole documents (pre-order DFS of the spine) — later slice.
pub fn render_document() -> Result<String> {
    unimplemented!("document render")
}

// === canonical inline form (format §7) =======================================
//
// Inline content is stored as a single string of *canonical* Karanga Markdown
// inline syntax. The canonical form is whatever this writer emits from the
// parsed event stream: `**strong**`, `*em*`, `~~strike~~`, backtick code
// spans, `[text](dest)` links (a `krg://` dest is an internal ref), single
// spaces for soft/hard breaks, and backslash-escaped markup characters in
// plain text. Normalization (parse → re-emit) is idempotent; hashing the
// stored string is therefore deterministic.

/// Builds the canonical inline string from `pulldown-cmark` inline events.
struct InlineWriter {
    out: String,
    /// Destination stack for open links.
    links: Vec<String>,
}

impl InlineWriter {
    fn new() -> Self {
        InlineWriter { out: String::new(), links: Vec::new() }
    }

    /// Consume one event. Returns `false` if the event is not inline content
    /// (a block boundary) — the caller stops feeding.
    fn event(&mut self, ev: &Event) -> bool {
        match ev {
            Event::Text(s) => self.text(s),
            Event::Code(s) => self.code_span(s),
            Event::SoftBreak | Event::HardBreak => self.out.push(' '),
            Event::Start(Tag::Strong) => self.out.push_str("**"),
            Event::End(TagEnd::Strong) => self.out.push_str("**"),
            Event::Start(Tag::Emphasis) => self.out.push('*'),
            Event::End(TagEnd::Emphasis) => self.out.push('*'),
            Event::Start(Tag::Strikethrough) => self.out.push_str("~~"),
            Event::End(TagEnd::Strikethrough) => self.out.push_str("~~"),
            Event::Start(Tag::Link { dest_url, .. }) => {
                self.links.push(dest_url.to_string());
                self.out.push('[');
            }
            Event::End(TagEnd::Link) => {
                let dest = self.links.pop().unwrap_or_default();
                self.out.push_str("](");
                self.out.push_str(&link_dest(&dest));
                self.out.push(')');
            }
            _ => return false,
        }
        true
    }

    /// Escaped plain text. Block-introducing characters are additionally
    /// escaped at the very start of the string so a stored paragraph can never
    /// re-parse as a heading/quote/list when fed back through the dialect.
    fn text(&mut self, s: &str) {
        for (i, c) in s.char_indices() {
            let escape = match c {
                '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '~' => true,
                '&' => entity_follows(&s[i..]),
                '#' | '>' | '-' | '+' => self.out.is_empty(),
                '.' | ')' => {
                    // "1. " / "1) " at string start would re-parse as a list
                    // item. Everything emitted so far being digits is checked
                    // against the output, so it spans split text events.
                    !self.out.is_empty() && self.out.chars().all(|d| d.is_ascii_digit())
                }
                _ => false,
            };
            if escape {
                self.out.push('\\');
            }
            self.out.push(c);
        }
    }

    /// A code span, with a fence longer than any backtick run inside it and
    /// space padding when the content begins/ends with a backtick.
    fn code_span(&mut self, s: &str) {
        let longest = s
            .split(|c| c != '`')
            .map(str::len)
            .max()
            .unwrap_or(0);
        let fence = "`".repeat(longest + 1);
        let pad = s.starts_with('`') || s.ends_with('`');
        self.out.push_str(&fence);
        if pad {
            self.out.push(' ');
        }
        self.out.push_str(s);
        if pad {
            self.out.push(' ');
        }
        self.out.push_str(&fence);
    }

    fn finish(self) -> String {
        self.out
    }
}

/// True when `s` (starting with `&`) begins a character entity reference,
/// which CommonMark would decode; a bare ampersand needs no escape.
fn entity_follows(s: &str) -> bool {
    let rest = &s[1..];
    let body: String = rest.chars().take_while(|c| *c != ';').collect();
    if !rest[body.len()..].starts_with(';') || body.is_empty() {
        return false;
    }
    if let Some(num) = body.strip_prefix('#') {
        let hex = num.strip_prefix(['x', 'X']);
        return match hex {
            Some(h) => !h.is_empty() && h.chars().all(|c| c.is_ascii_hexdigit()),
            None => !num.is_empty() && num.chars().all(|c| c.is_ascii_digit()),
        };
    }
    body.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && body.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Wrap a link destination in `<…>` when it contains characters that would
/// break the bare `(dest)` form.
fn link_dest(dest: &str) -> String {
    if dest.chars().any(|c| c == ' ' || c == '(' || c == ')') {
        format!("<{dest}>")
    } else {
        dest.to_string()
    }
}

/// Normalize an inline Karanga Markdown fragment to its canonical form
/// (format §7): parse, then re-emit through the canonical writer. Block
/// structure in the input is ignored — text is concatenated.
pub fn normalize_inline(md: &str) -> String {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    let mut w = InlineWriter::new();
    for ev in Parser::new_ext(md, opts) {
        let _ = w.event(&ev); // non-inline events are simply skipped
    }
    w.finish()
}

/// The plaintext projection of inline content (format §7): markup stripped,
/// breaks collapsed to spaces.
pub fn strip_inline(md: &str) -> String {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    let mut s = String::new();
    for ev in Parser::new_ext(md, opts) {
        match ev {
            Event::Text(t) | Event::Code(t) => s.push_str(&t),
            Event::SoftBreak | Event::HardBreak => s.push(' '),
            _ => {}
        }
    }
    s
}

// === block-level parsing (authoring whole Markdown fragments) ===============

/// A parsed block: a node spec (no id yet) plus its child subtree. The caller
/// assigns ids and builds spine entries when inserting.
#[derive(Debug, Clone)]
pub struct Block {
    pub ty: String,
    /// Canonical inline markdown for text-bearing types, raw text for `code`,
    /// `None` for containers/divider.
    pub content: Option<String>,
    pub attrs: Attrs,
    pub children: Vec<Block>,
}

/// Parse a Karanga Markdown fragment into a tree of blocks (interface §8).
/// Handles headings (with section nesting), paragraphs, code, blockquotes,
/// lists (nested), and dividers; tables and HTML are skipped for v0.1 import.
pub fn parse_markdown(md: &str) -> Vec<Block> {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TABLES);
    let events: Vec<Event> = Parser::new_ext(md, opts).collect();
    let mut i = 0;
    let flat = block_seq(&events, &mut i, Ctx::Root);
    section_nest(flat)
}

#[derive(Clone, Copy)]
enum Ctx {
    Root,
    Quote,
    Item,
}

fn ctx_ends(te: &TagEnd, ctx: Ctx) -> bool {
    match ctx {
        Ctx::Root => false,
        Ctx::Quote => matches!(te, TagEnd::BlockQuote(_)),
        Ctx::Item => matches!(te, TagEnd::Item),
    }
}

fn is_block_tag(t: &Tag) -> bool {
    matches!(
        t,
        Tag::Heading { .. }
            | Tag::Paragraph
            | Tag::CodeBlock(_)
            | Tag::BlockQuote(_)
            | Tag::List(_)
            | Tag::Item
            | Tag::Table(_)
            | Tag::TableHead
            | Tag::TableRow
            | Tag::TableCell
            | Tag::HtmlBlock
    )
}

fn block_seq(ev: &[Event], i: &mut usize, ctx: Ctx) -> Vec<Block> {
    let mut out = Vec::new();
    while *i < ev.len() {
        match &ev[*i] {
            Event::End(te) if ctx_ends(te, ctx) => break,
            Event::Rule => {
                *i += 1;
                out.push(empty_block("divider"));
            }
            Event::Start(tag) if is_block_tag(tag) => {
                let tag = tag.clone();
                *i += 1;
                match tag {
                    Tag::Heading { level, .. } => {
                        let inline = collect_inline(ev, i);
                        consume_end(ev, i);
                        let mut attrs = Attrs::new();
                        attrs.insert("level".into(), Value::Int(hlevel(level)));
                        out.push(Block {
                            ty: "heading".into(),
                            content: Some(inline),
                            attrs,
                            children: Vec::new(),
                        });
                    }
                    Tag::Paragraph => {
                        let inline = collect_inline(ev, i);
                        consume_end(ev, i);
                        out.push(inline_block("paragraph", inline));
                    }
                    Tag::CodeBlock(kind) => {
                        let code = collect_text(ev, i);
                        consume_end(ev, i);
                        let mut attrs = Attrs::new();
                        if let CodeBlockKind::Fenced(lang) = &kind {
                            if !lang.is_empty() {
                                attrs.insert("language".into(), Value::Str(lang.to_string()));
                            }
                        }
                        out.push(Block {
                            ty: "code".into(),
                            content: Some(code),
                            attrs,
                            children: Vec::new(),
                        });
                    }
                    Tag::BlockQuote(_) => {
                        let kids = block_seq(ev, i, Ctx::Quote);
                        consume_end(ev, i);
                        out.push(container("blockquote", section_nest(kids)));
                    }
                    Tag::List(start) => {
                        let items = list_items(ev, i);
                        consume_end(ev, i);
                        let mut attrs = Attrs::new();
                        attrs.insert("ordered".into(), Value::Bool(start.is_some()));
                        out.push(Block {
                            ty: "list".into(),
                            content: None,
                            attrs,
                            children: items,
                        });
                    }
                    _ => skip_block(ev, i), // tables / html: skipped for v0.1
                }
            }
            // anything else at block position is inline → an implicit paragraph
            _ => {
                let inline = collect_inline(ev, i);
                if inline.is_empty() {
                    *i += 1; // ensure progress on unhandled events
                } else {
                    out.push(inline_block("paragraph", inline));
                }
            }
        }
    }
    out
}

fn list_items(ev: &[Event], i: &mut usize) -> Vec<Block> {
    let mut items = Vec::new();
    while *i < ev.len() {
        match &ev[*i] {
            Event::Start(Tag::Item) => {
                *i += 1;
                let kids = block_seq(ev, i, Ctx::Item);
                consume_end(ev, i);
                items.push(container("list-item", section_nest(kids)));
            }
            _ => break,
        }
    }
    items
}

/// Collect one block's inline events into the canonical inline string.
fn collect_inline(ev: &[Event], i: &mut usize) -> String {
    let mut w = InlineWriter::new();
    while *i < ev.len() {
        if !w.event(&ev[*i]) {
            break; // a block boundary
        }
        *i += 1;
    }
    w.finish()
}

fn collect_text(ev: &[Event], i: &mut usize) -> String {
    let mut s = String::new();
    while *i < ev.len() {
        match &ev[*i] {
            Event::Text(t) | Event::Code(t) => {
                s.push_str(t);
                *i += 1;
            }
            _ => break,
        }
    }
    s.strip_suffix('\n').map(str::to_string).unwrap_or(s)
}

/// Skip a Start tag we don't model, through its matching End.
fn skip_block(ev: &[Event], i: &mut usize) {
    let mut depth = 1;
    while *i < ev.len() && depth > 0 {
        match &ev[*i] {
            Event::Start(_) => depth += 1,
            Event::End(_) => depth -= 1,
            _ => {}
        }
        *i += 1;
    }
}

fn consume_end(ev: &[Event], i: &mut usize) {
    if matches!(ev.get(*i), Some(Event::End(_))) {
        *i += 1;
    }
}

/// Fold a flat block list into a tree, nesting blocks under preceding headings
/// by level (heading-as-container, format §5.3).
fn section_nest(flat: Vec<Block>) -> Vec<Block> {
    let mut roots: Vec<Block> = Vec::new();
    let mut stack: Vec<Block> = Vec::new();
    for b in flat {
        if b.ty == "heading" {
            let level = heading_level(&b);
            while stack.last().is_some_and(|t| heading_level(t) >= level) {
                let done = stack.pop().unwrap();
                attach(&mut stack, &mut roots, done);
            }
            stack.push(b);
        } else {
            attach(&mut stack, &mut roots, b);
        }
    }
    while let Some(done) = stack.pop() {
        attach(&mut stack, &mut roots, done);
    }
    roots
}

fn attach(stack: &mut [Block], roots: &mut Vec<Block>, b: Block) {
    if let Some(top) = stack.last_mut() {
        top.children.push(b);
    } else {
        roots.push(b);
    }
}

fn heading_level(b: &Block) -> i64 {
    match b.attrs.get("level") {
        Some(Value::Int(n)) => *n,
        _ => 1,
    }
}

fn hlevel(l: HeadingLevel) -> i64 {
    use HeadingLevel::*;
    match l {
        H1 => 1,
        H2 => 2,
        H3 => 3,
        H4 => 4,
        H5 => 5,
        H6 => 6,
    }
}

fn empty_block(ty: &str) -> Block {
    Block { ty: ty.into(), content: None, attrs: Attrs::new(), children: Vec::new() }
}

fn inline_block(ty: &str, inline: String) -> Block {
    Block { ty: ty.into(), content: Some(inline), attrs: Attrs::new(), children: Vec::new() }
}

fn container(ty: &str, children: Vec<Block>) -> Block {
    Block { ty: ty.into(), content: None, attrs: Attrs::new(), children }
}

fn attr_str<'a>(node: &'a Node, key: &str) -> Option<&'a str> {
    match node.attrs.get(key) {
        Some(Value::Str(s)) => Some(s.as_str()),
        _ => None,
    }
}

fn attr_int(node: &Node, key: &str) -> Option<i64> {
    match node.attrs.get(key) {
        Some(Value::Int(i)) => Some(*i),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize_inline, strip_inline};

    /// The canonical form is a fixpoint of the normalizer — required for
    /// deterministic hashing (format §7, §9).
    #[test]
    fn normalize_is_idempotent() {
        let inputs = [
            "plain text",
            "a *em* **strong** ~~strike~~ `code` mix",
            "literal 2*3=6, snake_case, [brackets], a\\backslash, `tick",
            "[ext](https://example.com/a) and [ref](krg:///n1)",
            "AT&T stays bare but &amp; is escaped",
            "#not-a-heading and - dash first",
            "1. not a list",
            "nested ***both*** and **outer *inner* rest**",
        ];
        for md in inputs {
            let once = normalize_inline(md);
            assert_eq!(normalize_inline(&once), once, "not a fixpoint for {md:?}");
        }
    }

    /// Markup characters in literal prose are escaped in the stored form and
    /// recovered exactly by the plaintext projection.
    #[test]
    fn escaping_round_trips_literal_text() {
        let cases = [
            ("2\\*3=6 and snake\\_case", "2*3=6 and snake_case"),
            ("\\[not a link\\]", "[not a link]"),
            ("&amp; is an ampersand", "& is an ampersand"),
        ];
        for (md, plain) in cases {
            let canon = normalize_inline(md);
            assert_eq!(strip_inline(&canon), plain);
            assert_eq!(normalize_inline(&canon), canon);
        }
    }

    /// Backticks inside code spans get a longer fence and padding.
    #[test]
    fn code_span_fencing() {
        let canon = normalize_inline("``a ` b``");
        assert_eq!(canon, "``a ` b``");
        assert_eq!(strip_inline(&canon), "a ` b");
    }

    /// Block-introducing characters at the start of a paragraph's text are
    /// escaped so the stored string cannot re-parse as a different block.
    #[test]
    fn leading_block_chars_are_escaped() {
        assert_eq!(normalize_inline("\\- dash first"), "\\- dash first");
        assert_eq!(normalize_inline("#x"), "\\#x");
        assert_eq!(normalize_inline("1\\. not a list"), "1\\. not a list");
    }

    /// The plaintext projection strips marks and link syntax.
    #[test]
    fn strip_removes_markup() {
        assert_eq!(
            strip_inline("a **b** *c* ~~d~~ `e` [f](https://x) g"),
            "a b c d e f g"
        );
    }
}
