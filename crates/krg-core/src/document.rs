//! An opened document: a [`Store`] + parsed manifest + spine. Read + render path.
//!
//! `Document::open` accepts a packed `.krg` (via [`ZipStore`]) or an exploded
//! directory (via [`DirStore`]). The Ref-based verbs in [`crate::query`] will
//! wrap this once cross-document discovery exists.

use std::collections::BTreeSet;
use std::path::Path;

use crate::container::{DirStore, Store, ZipStore};
use crate::error::Error;
use crate::hash::{content_hash, rev_of};
use crate::id::{NodeId, Ref, Rev};
use crate::model::{Link, Links, Manifest, Node, NodeContent, Spine, SpineEntry, Value};
use crate::query::Direction;
use crate::render;
use crate::tree::EditorBlock;
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
        // Media needs store access to resolve an embedded asset to its path;
        // other leaf types render locally.
        let content = if entry.ty == "media" {
            self.render_media(entry)?
        } else {
            render::render_node(&self.read_node(id)?)
        };
        Ok(RenderedNode {
            r: Ref::node(&self.manifest.doc_id, &NodeId(id.to_string())),
            ty: entry.ty.clone(),
            rev: rev_of(&entry.hash),
            content,
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

    /// Conformance check (format §11): recompute each node's content hash and
    /// compare to its spine projection, verify the spine↔nodes bijection, and
    /// detect duplicate ids and type-projection drift. Returns the list of
    /// issues (empty ⇒ valid).
    pub fn validate(&self) -> Result<Vec<String>> {
        let mut issues = Vec::new();
        let mut ids = BTreeSet::new();
        for e in &self.spine.root {
            self.validate_entry(e, &mut ids, &mut issues);
        }
        let parts: BTreeSet<String> = self
            .store
            .list("nodes")?
            .into_iter()
            .filter_map(|p| {
                Path::new(&p)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(String::from)
            })
            .collect();
        for id in &ids {
            if !parts.contains(id) {
                issues.push(format!("spine references missing node part '{id}'"));
            }
        }
        for p in &parts {
            if !ids.contains(p) {
                issues.push(format!("orphan node part '{p}' not in spine"));
            }
        }
        Ok(issues)
    }

    fn validate_entry(
        &self,
        e: &SpineEntry,
        ids: &mut BTreeSet<String>,
        issues: &mut Vec<String>,
    ) {
        if !ids.insert(e.id.0.clone()) {
            issues.push(format!("duplicate node id '{}'", e.id.0));
        }
        match self.read_node(&e.id.0) {
            Ok(node) => {
                if node.ty != e.ty {
                    issues.push(format!(
                        "type projection drift for '{}': spine='{}' node='{}'",
                        e.id.0, e.ty, node.ty
                    ));
                }
                match content_hash(&node) {
                    Ok(h) if h != e.hash => issues.push(format!(
                        "hash mismatch for '{}': spine={} actual={}",
                        e.id.0, e.hash, h
                    )),
                    Ok(_) => {}
                    Err(err) => issues.push(format!("hash error for '{}': {err}", e.id.0)),
                }
            }
            Err(err) => issues.push(format!("cannot read node '{}': {err}", e.id.0)),
        }
        for c in &e.children {
            self.validate_entry(c, ids, issues);
        }
    }

    /// Produce the editor document tree (engine → WYSIWYG client). Inline
    /// content is rendered to Karanga Markdown; container children nest.
    pub fn to_tree(&self) -> Result<Vec<EditorBlock>> {
        self.spine.root.iter().map(|e| self.block_of(e)).collect()
    }

    fn block_of(&self, e: &SpineEntry) -> Result<EditorBlock> {
        let node = self.read_node(&e.id.0)?;
        let content = match &node.content {
            NodeContent::Inline(_) => render::inline_markdown(&node),
            NodeContent::Raw(s) => s.clone(),
            NodeContent::Empty => String::new(),
        };
        let children = e
            .children
            .iter()
            .map(|c| self.block_of(c))
            .collect::<Result<Vec<_>>>()?;
        Ok(EditorBlock {
            id: Some(e.id.0.clone()),
            ty: node.ty.clone(),
            content,
            attrs: node.attrs.clone(),
            children,
        })
    }

    /// Nodes filtered by segment type (from the spine; no body reads).
    /// Returns `(node_id, type, label)`.
    pub fn find_nodes(&self, ty: Option<&str>) -> Vec<(String, String, Option<String>)> {
        let mut out = Vec::new();
        fn walk(e: &SpineEntry, ty: Option<&str>, out: &mut Vec<(String, String, Option<String>)>) {
            if ty.is_none_or(|t| t == e.ty) {
                out.push((e.id.0.clone(), e.ty.clone(), e.label.clone()));
            }
            for c in &e.children {
                walk(c, ty, out);
            }
        }
        for e in &self.spine.root {
            walk(e, ty, &mut out);
        }
        out
    }

    /// All links recorded in `links.json` (empty if absent).
    pub fn links(&self) -> Result<Vec<Link>> {
        match self.store.read_part("links.json") {
            Ok(bytes) => serde_json::from_slice::<Links>(&bytes)
                .map(|l| l.links)
                .map_err(|e| Error::Parse(format!("links.json: {e}"))),
            Err(_) => Ok(Vec::new()),
        }
    }

    /// Links touching a node, by direction (interface §3.7).
    pub fn get_links(&self, node_id: &str, dir: Direction) -> Result<Vec<Link>> {
        let full = Ref::node(&self.manifest.doc_id, &NodeId(node_id.to_string())).0;
        let short = format!("krg:///{node_id}");
        let mut out = Vec::new();
        for l in self.links()? {
            let is_out = l.from.0 == node_id;
            let is_in = l.to.0 == full || l.to.0 == short;
            let keep = match dir {
                Direction::Out => is_out,
                Direction::In => is_in,
                Direction::Both => is_out || is_in,
            };
            if keep {
                out.push(l);
            }
        }
        Ok(out)
    }

    /// `(node_id, type, plaintext)` for every node carrying text — the input
    /// to full-text indexing/search.
    pub fn plaintext_nodes(&self) -> Result<Vec<(String, String, String)>> {
        let mut out = Vec::new();
        for e in &self.spine.root {
            self.collect_text(e, &mut out)?;
        }
        Ok(out)
    }

    fn collect_text(&self, e: &SpineEntry, out: &mut Vec<(String, String, String)>) -> Result<()> {
        let text = node_plaintext(&self.read_node(&e.id.0)?);
        if !text.is_empty() {
            out.push((e.id.0.clone(), e.ty.clone(), text));
        }
        for c in &e.children {
            self.collect_text(c, out)?;
        }
        Ok(())
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

fn node_plaintext(node: &Node) -> String {
    match &node.content {
        NodeContent::Inline(runs) => runs.iter().map(|r| r.text.as_str()).collect(),
        NodeContent::Raw(s) => s.clone(),
        NodeContent::Empty => String::new(),
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
