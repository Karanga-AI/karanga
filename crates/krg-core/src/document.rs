//! An opened document: a [`Store`] + parsed manifest + spine. The read path.
//!
//! `Document::open` accepts a packed `.krg` (via [`ZipStore`]) or an exploded
//! directory (via [`DirStore`]). The Ref-based verbs in [`crate::query`] will
//! wrap this once cross-document discovery exists.

use std::path::Path;

use crate::container::{DirStore, Store, ZipStore};
use crate::error::Error;
use crate::hash::rev_of;
use crate::id::{NodeId, Ref, Rev};
use crate::model::{Manifest, Node, Spine, SpineEntry};
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
        let raw = self.store.read_part(&format!("nodes/{id}.json"))?;
        let node: Node = serde_json::from_slice(&raw)
            .map_err(|e| Error::Parse(format!("nodes/{id}.json: {e}")))?;
        Ok(RenderedNode {
            r: Ref::node(&self.manifest.doc_id, &NodeId(id.to_string())),
            ty: entry.ty.clone(),
            rev: rev_of(&entry.hash),
            content: render::render_node(&node),
        })
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
