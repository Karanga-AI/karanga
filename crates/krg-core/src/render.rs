//! Schema-driven projection (read side) and authoring parse (write side).
//!
//! Read: nodes → Karanga Markdown (interface §3, §8). This slice renders a
//! single node's own content; whole-section/document rendering (walking the
//! spine, container children) lands in a later slice.

use std::collections::BTreeMap;

use crate::model::{MarkDef, Node, NodeContent, Value};
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

/// Parse Karanga Markdown into nodes (authoring inverse) — later slice.
pub fn parse_markdown(_md: &str) -> Result<Vec<Node>> {
    unimplemented!("Karanga Markdown parse")
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
