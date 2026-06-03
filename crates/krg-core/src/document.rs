//! An opened document: a [`Store`] + parsed manifest + spine. Read + render path.
//!
//! `Document::open` accepts a packed `.krg` (via [`ZipStore`]) or an exploded
//! directory (via [`DirStore`]). The Ref-based verbs in [`crate::query`] will
//! wrap this once cross-document discovery exists.

use std::path::Path;

use crate::container::{DirStore, Store, ZipStore};
use crate::error::Error;
use crate::hash::rev_of;
use crate::id::{NodeId, Ref, Rev};
use crate::model::{Manifest, Node, Spine, SpineEntry, Value};
use crate::render;
use crate::Result;

pub struct Document {
    store: Box<dyn Store>,
    pub manifest: Manifest,
    pub spine: Spine,
}

/// Tier-3 result for a single node.
pub struct RenderedNode {
    pub r: Ref,
    pub ty: String,
    pub rev: Rev,
    pub content: String,
}

impl Document {
    /// Open a `.krg` file or an exploded document directory.
    pub fn open(path: impl AsRef<Path>) -> Result<Document> {
        let path = path.as_ref();
        let store: Box<dyn Store> = if path.is_dir() {
            Box::new(DirStore::open(path))
        } else {
            Box::new(ZipStore::open(path))
        };
        let manifest: Manifest = serde_json::from_slice(&store.read_part("manifest.json")?)
            .map_err(|e| Error::Parse(format!("manifest.json: {e}")))?;
        let spine: Spine = serde_json::from_slice(&store.read_part("spine.json")?)
            .map_err(|e| Error::Parse(format!("spine.json: {e}")))?;
        Ok(Document { store, manifest, spine })
    }

    /// Tier 2 — the outline (headings only, nested by heading depth).
    pub fn outline(&self) -> String {
        let mut out = format!(
            "{}   {}\n",
            self.manifest.title,
            Ref::document(&self.manifest.doc_id).0
        );
        for e in &self.spine.root {
            outline_walk(e, 0, &mut out);
        }
        out
    }

    /// Tier 3 — one rendered node plus its `rev` (taken from the spine hash).
    pub fn node(&self, id: &str) -> Result<RenderedNode> {
        let entry = find(&self.spine.root, id)
            .ok_or_else(|| Error::NotFound(format!("node '{id}'")))?;
        Ok(RenderedNode {
            r: Ref::node(&self.manifest.doc_id, &NodeId(id.to_string())),
            ty: entry.ty.clone(),
            rev: rev_of(&entry.hash),
            content: render::render_node(&self.read_node(id)?),
        })
    }

    /// Render the whole document to Karanga Markdown (pre-order DFS of the spine).
    pub fn render(&self) -> Result<String> {
        let mut blocks = Vec::new();
        for e in &self.spine.root {
            self.emit(e, &mut blocks)?;
        }
        Ok(format!("{}\n", blocks.join("\n\n")))
    }

    /// Render a section subtree rooted at the given node (e.g. a heading).
    pub fn section(&self, id: &str) -> Result<String> {
        let entry = find(&self.spine.root, id)
            .ok_or_else(|| Error::NotFound(format!("node '{id}'")))?;
        let mut blocks = Vec::new();
        self.emit(entry, &mut blocks)?;
        Ok(format!("{}\n", blocks.join("\n\n")))
    }

    fn read_node(&self, id: &str) -> Result<Node> {
        let raw = self.store.read_part(&format!("nodes/{id}.json"))?;
        serde_json::from_slice(&raw).map_err(|e| Error::Parse(format!("nodes/{id}.json: {e}")))
    }

    /// Emit one entry as one or more blocks in reading order. Headings *flow*
    /// their section children as following blocks; lists/quotes/tables/custom
    /// containers *absorb* their children into a composed block.
    fn emit(&self, e: &SpineEntry, out: &mut Vec<String>) -> Result<()> {
        match e.ty.as_str() {
            "heading" => {
                out.push(render::render_node(&self.read_node(&e.id.0)?));
                for c in &e.children {
                    self.emit(c, out)?;
                }
            }
            "blockquote" => out.push(self.render_blockquote(e)?),
            "list" => out.push(self.render_list(e)?),
            "table" => out.push(self.render_table(e)?),
            "media" => out.push(self.render_media(e)?),
            t if t.contains(':') => out.push(self.render_directive(e)?),
            _ => out.push(render::render_node(&self.read_node(&e.id.0)?)),
        }
        Ok(())
    }

    fn render_blockquote(&self, e: &SpineEntry) -> Result<String> {
        let mut inner = Vec::new();
        for c in &e.children {
            self.emit(c, &mut inner)?;
        }
        let body = inner.join("\n\n");
        Ok(body
            .lines()
            .map(|l| if l.is_empty() { ">".to_string() } else { format!("> {l}") })
            .collect::<Vec<_>>()
            .join("\n"))
    }

    fn render_list(&self, e: &SpineEntry) -> Result<String> {
        let node = self.read_node(&e.id.0)?;
        let ordered = matches!(node.attrs.get("ordered"), Some(Value::Bool(true)));
        let mut items = Vec::new();
        for (i, item) in e.children.iter().enumerate() {
            let marker = if ordered { format!("{}. ", i + 1) } else { "- ".to_string() };
            let indent = " ".repeat(marker.len());
            let mut blocks = Vec::new();
            for c in &item.children {
                self.emit(c, &mut blocks)?;
            }
            let mut md = String::new();
            for (bi, blk) in blocks.iter().enumerate() {
                if bi == 0 {
                    let mut lines = blk.lines();
                    if let Some(first) = lines.next() {
                        md.push_str(&format!("{marker}{first}"));
                    }
                    for l in lines {
                        md.push('\n');
                        md.push_str(&format!("{indent}{l}"));
                    }
                } else {
                    md.push('\n');
                    md.push_str(&indent_block(blk, &indent));
                }
            }
            items.push(md);
        }
        Ok(items.join("\n"))
    }

    fn render_table(&self, e: &SpineEntry) -> Result<String> {
        let node = self.read_node(&e.id.0)?;
        let aligns: Vec<String> = match node.attrs.get("align") {
            Some(Value::List(l)) => l
                .iter()
                .filter_map(|v| if let Value::Str(s) = v { Some(s.clone()) } else { None })
                .collect(),
            _ => Vec::new(),
        };
        let mut rows: Vec<Vec<String>> = Vec::new();
        for row in &e.children {
            let mut cells = Vec::new();
            for cell in &row.children {
                cells.push(render::render_node(&self.read_node(&cell.id.0)?));
            }
            rows.push(cells);
        }
        if rows.is_empty() {
            return Ok(String::new());
        }
        let ncol = rows[0].len();
        let mut out = vec![format!("| {} |", rows[0].join(" | "))];
        let sep: Vec<String> = (0..ncol)
            .map(|c| match aligns.get(c).map(String::as_str) {
                Some("left") => ":---".to_string(),
                Some("center") => ":---:".to_string(),
                Some("right") => "---:".to_string(),
                _ => "---".to_string(),
            })
            .collect();
        out.push(format!("| {} |", sep.join(" | ")));
        for r in &rows[1..] {
            out.push(format!("| {} |", r.join(" | ")));
        }
        Ok(out.join("\n"))
    }

    fn render_media(&self, e: &SpineEntry) -> Result<String> {
        let node = self.read_node(&e.id.0)?;
        let alt = attr_str(&node, "alt").unwrap_or("");
        let src = if let Some(s) = attr_str(&node, "src") {
            s.to_string()
        } else if let Some(asset) = attr_str(&node, "asset") {
            self.resolve_asset(asset).unwrap_or_else(|| format!("media/{asset}"))
        } else {
            String::new()
        };
        let mut s = format!("![{alt}]({src})");
        if let Some(cap) = attr_str(&node, "caption") {
            s.push_str(&format!("\n\n*{cap}*"));
        }
        Ok(s)
    }

    fn resolve_asset(&self, asset: &str) -> Option<String> {
        let names = self.store.list("media").ok()?;
        names
            .into_iter()
            .find(|n| Path::new(n).file_stem().and_then(|s| s.to_str()) == Some(asset))
    }

    fn render_directive(&self, e: &SpineEntry) -> Result<String> {
        let node = self.read_node(&e.id.0)?;
        let attrs = fmt_attrs(&node);
        let mut inner = Vec::new();
        for c in &e.children {
            self.emit(c, &mut inner)?;
        }
        let body = inner.join("\n\n");
        Ok(format!(":::{}{}\n{}\n:::", e.ty, attrs, body))
    }
}

fn find<'a>(entries: &'a [SpineEntry], id: &str) -> Option<&'a SpineEntry> {
    for e in entries {
        if e.id.0 == id {
            return Some(e);
        }
        if let Some(found) = find(&e.children, id) {
            return Some(found);
        }
    }
    None
}

fn outline_walk(e: &SpineEntry, depth: usize, out: &mut String) {
    let mut depth = depth;
    if e.ty == "heading" {
        let label = e.label.clone().unwrap_or_default();
        out.push_str(&format!("{}- {}  ⟨{}⟩\n", "  ".repeat(depth), label, e.id.0));
        depth += 1;
    }
    for c in &e.children {
        outline_walk(c, depth, out);
    }
}

fn attr_str<'a>(node: &'a Node, key: &str) -> Option<&'a str> {
    match node.attrs.get(key) {
        Some(Value::Str(s)) => Some(s.as_str()),
        _ => None,
    }
}

fn indent_block(s: &str, indent: &str) -> String {
    s.lines()
        .map(|l| format!("{indent}{l}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn fmt_attrs(node: &Node) -> String {
    if node.attrs.is_empty() {
        return String::new();
    }
    let parts: Vec<String> = node
        .attrs
        .iter()
        .map(|(k, v)| match v {
            Value::Str(s) => format!("{k}=\"{s}\""),
            Value::Int(i) => format!("{k}={i}"),
            Value::Bool(b) => format!("{k}={b}"),
            _ => format!("{k}=?"),
        })
        .collect();
    format!("{{{}}}", parts.join(" "))
}
