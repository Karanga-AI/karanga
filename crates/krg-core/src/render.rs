//! Schema-driven projection (read side) and authoring parse (write side).
//!
//! Read: nodes → Karanga Markdown (interface §3, §8). This slice renders a
//! single node's own content; whole-section/document rendering (walking the
//! spine, container children) lands in a later slice.

use std::collections::BTreeMap;

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use crate::id::Ref;
use crate::model::{Attrs, MarkDef, Node, NodeContent, Run, Value};
use crate::Result;

/// Render a single node to Karanga Markdown.
pub fn render_node(node: &Node) -> String {
    match node.ty.as_str() {
        "heading" => {
            let level = attr_int(node, "level").unwrap_or(1).clamp(1, 6) as usize;
            format!("{} {}", "#".repeat(level), inline(node))
        }
        "paragraph" => inline(node),
        "table-cell" => inline(node),
        "code" => {
            let lang = attr_str(node, "language").unwrap_or("");
            let body = match &node.content {
                NodeContent::Raw(s) => s.as_str(),
                _ => "",
            };
            format!("```{lang}\n{body}\n```")
        }
        "divider" => "---".to_string(),
        // Containers: own content is empty; children are rendered by section/
        // document rendering (later slice). Render a minimal shell.
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
        _ => inline(node), // list / list-item / table / table-row → empty own content
    }
}

/// Render whole documents (pre-order DFS of the spine) — later slice.
pub fn render_document() -> Result<String> {
    unimplemented!("document render")
}

/// Parse the inline content of a single text-bearing block (paragraph, heading,
/// table-cell) into runs + a node-local mark table. Uses `pulldown-cmark`;
/// block structure is ignored (the caller supplies inline content).
pub fn parse_inline(md: &str) -> (Vec<Run>, BTreeMap<String, MarkDef>) {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    let mut runs = Vec::new();
    let mut marks = BTreeMap::new();
    let mut active: Vec<String> = Vec::new();
    let mut link: Option<String> = None;
    let mut ctr = 0u32;

    for ev in Parser::new_ext(md, opts) {
        match ev {
            Event::Start(Tag::Strong) => active.push("strong".into()),
            Event::End(TagEnd::Strong) => pop_mark(&mut active, "strong"),
            Event::Start(Tag::Emphasis) => active.push("em".into()),
            Event::End(TagEnd::Emphasis) => pop_mark(&mut active, "em"),
            Event::Start(Tag::Strikethrough) => active.push("strike".into()),
            Event::End(TagEnd::Strikethrough) => pop_mark(&mut active, "strike"),
            Event::Start(Tag::Link { dest_url, .. }) => {
                ctr += 1;
                let key = format!("m{ctr}");
                let def = if dest_url.starts_with("krg://") {
                    MarkDef { ty: "ref".into(), href: None, target: Some(Ref(dest_url.to_string())) }
                } else {
                    MarkDef { ty: "link".into(), href: Some(dest_url.to_string()), target: None }
                };
                marks.insert(key.clone(), def);
                link = Some(key);
            }
            Event::End(TagEnd::Link) => link = None,
            Event::Code(s) => push_run(&mut runs, s.to_string(), &active, Some("code"), &link),
            Event::Text(s) => push_run(&mut runs, s.to_string(), &active, None, &link),
            Event::SoftBreak | Event::HardBreak => {
                push_run(&mut runs, " ".to_string(), &active, None, &link)
            }
            _ => {}
        }
    }
    (runs, marks)
}

fn pop_mark(active: &mut Vec<String>, m: &str) {
    if let Some(p) = active.iter().rposition(|x| x == m) {
        active.remove(p);
    }
}

fn push_run(runs: &mut Vec<Run>, text: String, active: &[String], extra: Option<&str>, link: &Option<String>) {
    let mut marks: Vec<String> = active.to_vec();
    if let Some(e) = extra {
        marks.push(e.to_string());
    }
    if let Some(k) = link {
        marks.push(k.clone());
    }
    runs.push(Run { text, marks });
}

// === block-level parsing (authoring whole Markdown fragments) ===============

/// A parsed block: a node spec (no id yet) plus its child subtree. The caller
/// assigns ids and builds spine entries when inserting.
#[derive(Debug, Clone)]
pub struct Block {
    pub ty: String,
    pub content: NodeContent,
    pub attrs: Attrs,
    pub marks: BTreeMap<String, MarkDef>,
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
                        let (runs, marks) = collect_inline(ev, i);
                        consume_end(ev, i);
                        let mut attrs = Attrs::new();
                        attrs.insert("level".into(), Value::Int(hlevel(level)));
                        out.push(Block {
                            ty: "heading".into(),
                            content: NodeContent::Inline(runs),
                            attrs,
                            marks,
                            children: Vec::new(),
                        });
                    }
                    Tag::Paragraph => {
                        let (runs, marks) = collect_inline(ev, i);
                        consume_end(ev, i);
                        out.push(inline_block("paragraph", runs, marks));
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
                            content: NodeContent::Raw(code),
                            attrs,
                            marks: BTreeMap::new(),
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
                            content: NodeContent::Empty,
                            attrs,
                            marks: BTreeMap::new(),
                            children: items,
                        });
                    }
                    _ => skip_block(ev, i), // tables / html: skipped for v0.1
                }
            }
            // anything else at block position is inline → an implicit paragraph
            _ => {
                let (runs, marks) = collect_inline(ev, i);
                if runs.is_empty() {
                    *i += 1; // ensure progress on unhandled events
                } else {
                    out.push(inline_block("paragraph", runs, marks));
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

fn collect_inline(ev: &[Event], i: &mut usize) -> (Vec<Run>, BTreeMap<String, MarkDef>) {
    let mut runs = Vec::new();
    let mut marks = BTreeMap::new();
    let mut active: Vec<String> = Vec::new();
    let mut link: Option<String> = None;
    let mut ctr = 0u32;
    while *i < ev.len() {
        match &ev[*i] {
            Event::Text(s) => push_run(&mut runs, s.to_string(), &active, None, &link),
            Event::Code(s) => push_run(&mut runs, s.to_string(), &active, Some("code"), &link),
            Event::SoftBreak | Event::HardBreak => push_run(&mut runs, " ".into(), &active, None, &link),
            Event::Start(Tag::Strong) => active.push("strong".into()),
            Event::End(TagEnd::Strong) => pop_mark(&mut active, "strong"),
            Event::Start(Tag::Emphasis) => active.push("em".into()),
            Event::End(TagEnd::Emphasis) => pop_mark(&mut active, "em"),
            Event::Start(Tag::Strikethrough) => active.push("strike".into()),
            Event::End(TagEnd::Strikethrough) => pop_mark(&mut active, "strike"),
            Event::Start(Tag::Link { dest_url, .. }) => {
                ctr += 1;
                let key = format!("m{ctr}");
                let def = if dest_url.starts_with("krg://") {
                    MarkDef { ty: "ref".into(), href: None, target: Some(Ref(dest_url.to_string())) }
                } else {
                    MarkDef { ty: "link".into(), href: Some(dest_url.to_string()), target: None }
                };
                marks.insert(key.clone(), def);
                link = Some(key);
            }
            Event::End(TagEnd::Link) => link = None,
            _ => break, // a block boundary
        }
        *i += 1;
    }
    (runs, marks)
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
    Block { ty: ty.into(), content: NodeContent::Empty, attrs: Attrs::new(), marks: BTreeMap::new(), children: Vec::new() }
}

fn inline_block(ty: &str, runs: Vec<Run>, marks: BTreeMap<String, MarkDef>) -> Block {
    Block { ty: ty.into(), content: NodeContent::Inline(runs), attrs: Attrs::new(), marks, children: Vec::new() }
}

fn container(ty: &str, children: Vec<Block>) -> Block {
    Block { ty: ty.into(), content: NodeContent::Empty, attrs: Attrs::new(), marks: BTreeMap::new(), children }
}

fn inline(node: &Node) -> String {
    let runs = match &node.content {
        NodeContent::Inline(r) => r,
        _ => return String::new(),
    };
    let mut s = String::new();
    for run in runs {
        s.push_str(&apply_marks(&run.text, &run.marks, &node.marks));
    }
    s
}

fn apply_marks(text: &str, marks: &[String], defs: &BTreeMap<String, MarkDef>) -> String {
    let mut t = text.to_string();
    // simple marks, inner → outer
    if marks.iter().any(|m| m == "code") {
        t = format!("`{t}`");
    }
    if marks.iter().any(|m| m == "strike") {
        t = format!("~~{t}~~");
    }
    if marks.iter().any(|m| m == "em") {
        t = format!("*{t}*");
    }
    if marks.iter().any(|m| m == "strong") {
        t = format!("**{t}**");
    }
    // parametric marks (link/ref) wrap outermost
    for m in marks {
        if let Some(def) = defs.get(m) {
            match def.ty.as_str() {
                "link" => {
                    if let Some(h) = &def.href {
                        t = format!("[{t}]({h})");
                    }
                }
                "ref" => {
                    if let Some(target) = &def.target {
                        t = format!("[{t}]({})", target.0);
                    }
                }
                _ => {}
            }
        }
    }
    t
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
