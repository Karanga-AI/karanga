//! Schema-driven projection (read side) and authoring parse (write side).
//!
//! Read: nodes → Karanga Markdown (interface §3, §8). This slice renders a
//! single node's own content; whole-section/document rendering (walking the
//! spine, container children) lands in a later slice.

use std::collections::BTreeMap;

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use crate::id::Ref;
use crate::model::{MarkDef, Node, NodeContent, Run, Value};
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

/// Parse Karanga Markdown into nodes (block-level authoring inverse) — later slice.
pub fn parse_markdown(_md: &str) -> Result<Vec<Node>> {
    unimplemented!("block-level Karanga Markdown parse")
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
