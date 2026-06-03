//! Editing a document: an exploded working copy + the write verbs with
//! optimistic CAS (interface §4–§5; core-architecture §4.1, §6).
//!
//! A `Workspace` mutates the exploded form (a [`DirStore`]) and repacks to a
//! `.krg` on [`Workspace::save`]. Concurrency is optimistic: a mutation of an
//! existing node must present its current `rev`, or the write is reported
//! `Stale` (no last-writer-wins). v0.1 covers create / insert / update / delete;
//! move, links, and media writes are later work.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::container::{self, DirStore, Store};
use crate::error::Error;
use crate::hash::{content_hash, rev_of};
use crate::id::{DocId, NodeId, Ref, Rev};
use crate::model::{
    Attrs, Link, Links, Manifest, MarkDef, MediaMode, Node, NodeContent, Spine, SpineEntry, Value,
};
use crate::render;
use crate::Result;

/// Where to place a newly inserted node.
#[derive(Clone)]
pub enum Place {
    /// Append at the end of the document root.
    Root,
    /// Append at the end of the given parent node's children.
    Under(String),
}

/// Outcome of a CAS-guarded mutation.
pub enum Cas {
    /// Applied; carries the new `rev` for content mutations (none for delete).
    Ok(Option<Rev>),
    /// The on-disk node changed since `rev` was read; caller re-reads & retries.
    Stale { current_rev: Rev, current: String },
}

pub struct Workspace {
    dir: PathBuf,
    store: DirStore,
    manifest: Manifest,
    spine: Spine,
    links: Vec<Link>,
}

impl Workspace {
    /// Create a new, empty document in a working directory.
    pub fn create(dir: impl AsRef<Path>, title: &str, description: Option<&str>) -> Result<Workspace> {
        let dir = dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&dir).map_err(|e| Error::Io(e.to_string()))?;
        let manifest = Manifest {
            krg: crate::FORMAT_VERSION.to_string(),
            doc_id: DocId(uuid::Uuid::new_v4().to_string()),
            title: title.to_string(),
            description: description.map(String::from),
            created: None,
            modified: None,
            media_mode: MediaMode::Embedded,
            authors: Vec::new(),
            types: BTreeMap::new(),
            ext: BTreeMap::new(),
        };
        let mut ws = Workspace {
            store: DirStore::open(&dir),
            dir,
            manifest,
            spine: Spine { root: Vec::new() },
            links: Vec::new(),
        };
        ws.store.write_part("mimetype", container::MIMETYPE.as_bytes())?;
        ws.flush()?;
        Ok(ws)
    }

    /// Open an exploded working directory.
    pub fn open(dir: impl AsRef<Path>) -> Result<Workspace> {
        let dir = dir.as_ref().to_path_buf();
        let store = DirStore::open(&dir);
        let manifest = serde_json::from_slice(&store.read_part("manifest.json")?)
            .map_err(|e| Error::Parse(format!("manifest.json: {e}")))?;
        let spine = serde_json::from_slice(&store.read_part("spine.json")?)
            .map_err(|e| Error::Parse(format!("spine.json: {e}")))?;
        let links = match store.read_part("links.json") {
            Ok(bytes) => serde_json::from_slice::<Links>(&bytes)
                .map(|l| l.links)
                .unwrap_or_default(),
            Err(_) => Vec::new(),
        };
        Ok(Workspace { dir, store, manifest, spine, links })
    }

    /// Explode a packed `.krg` into `work_dir` and open it for editing.
    pub fn open_packed(krg: impl AsRef<Path>, work_dir: impl AsRef<Path>) -> Result<Workspace> {
        container::explode(krg.as_ref(), work_dir.as_ref())?;
        Workspace::open(work_dir)
    }

    pub fn doc_ref(&self) -> Ref {
        Ref::document(&self.manifest.doc_id)
    }

    /// Insert a new node. `content` is Karanga Markdown for inline types,
    /// raw text for `code`, and ignored for containers/divider/media.
    /// Returns the new node id and its `rev`.
    pub fn insert(
        &mut self,
        place: Place,
        ty: &str,
        content: &str,
        attrs: Attrs,
    ) -> Result<(String, Rev)> {
        let (content, marks) = build_content(ty, content);
        let node = Node {
            id: NodeId(ulid::Ulid::new().to_string()),
            ty: ty.to_string(),
            content,
            attrs,
            marks,
            ext: BTreeMap::new(),
        };
        self.add_built(place, node)
    }

    /// Insert a whole Karanga Markdown fragment as a node subtree under `place`.
    /// Returns the ids of the top-level nodes created.
    pub fn insert_markdown(&mut self, place: Place, md: &str) -> Result<Vec<String>> {
        let mut ids = Vec::new();
        for block in render::parse_markdown(md) {
            ids.push(self.insert_block(place.clone(), block)?);
        }
        Ok(ids)
    }

    fn insert_block(&mut self, place: Place, block: render::Block) -> Result<String> {
        let render::Block { ty, content, attrs, marks, children } = block;
        let node = Node {
            id: NodeId(ulid::Ulid::new().to_string()),
            ty,
            content,
            attrs,
            marks,
            ext: BTreeMap::new(),
        };
        let (id, _) = self.add_built(place, node)?;
        for child in children {
            self.insert_block(Place::Under(id.clone()), child)?;
        }
        Ok(id)
    }

    /// Write a fully-built node and add its spine entry at `place`.
    fn add_built(&mut self, place: Place, node: Node) -> Result<(String, Rev)> {
        let hash = content_hash(&node)?;
        let rev = rev_of(&hash);
        let label = (node.ty == "heading").then(|| plaintext(&node));
        self.store
            .write_part(&format!("nodes/{}.json", node.id.0), &to_pretty(&node)?)?;
        let entry = SpineEntry {
            id: node.id.clone(),
            ty: node.ty.clone(),
            hash,
            label,
            children: Vec::new(),
        };
        match place {
            Place::Root => self.spine.root.push(entry),
            Place::Under(pid) => {
                let parent = find_mut(&mut self.spine.root, &pid)
                    .ok_or_else(|| Error::NotFound(format!("parent '{pid}'")))?;
                parent.children.push(entry);
            }
        }
        self.flush()?;
        Ok((node.id.0, rev))
    }

    /// Replace a node's content (CAS-guarded on `rev`).
    pub fn update(&mut self, id: &str, content: &str, rev: &Rev) -> Result<Cas> {
        let current_hash = self.entry_hash(id)?;
        let current_rev = rev_of(&current_hash);
        if &current_rev != rev {
            let current = render::render_node(&self.read_node(id)?);
            return Ok(Cas::Stale { current_rev, current });
        }
        let old = self.read_node(id)?;
        let (content, marks) = build_content(&old.ty, content);
        let node = Node {
            id: NodeId(id.to_string()),
            ty: old.ty.clone(),
            content,
            attrs: old.attrs.clone(),
            marks,
            ext: old.ext.clone(),
        };
        let hash = content_hash(&node)?;
        let new_rev = rev_of(&hash);
        self.store
            .write_part(&format!("nodes/{id}.json"), &to_pretty(&node)?)?;
        if let Some(entry) = find_mut(&mut self.spine.root, id) {
            entry.hash = hash;
            if entry.ty == "heading" {
                entry.label = Some(plaintext(&node));
            }
        }
        self.flush()?;
        Ok(Cas::Ok(Some(new_rev)))
    }

    /// Delete a node and its subtree (CAS-guarded on `rev`).
    pub fn delete(&mut self, id: &str, rev: &Rev) -> Result<Cas> {
        let current_hash = self.entry_hash(id)?;
        let current_rev = rev_of(&current_hash);
        if &current_rev != rev {
            let current = render::render_node(&self.read_node(id)?);
            return Ok(Cas::Stale { current_rev, current });
        }
        let removed = remove_entry(&mut self.spine.root, id)
            .ok_or_else(|| Error::NotFound(format!("node '{id}'")))?;
        let mut ids = Vec::new();
        collect_ids(&removed, &mut ids);
        for nid in ids {
            let _ = self.store.remove_part(&format!("nodes/{nid}.json"));
        }
        self.flush()?;
        Ok(Cas::Ok(None))
    }

    /// Relocate a node (and its subtree) to a new place (CAS-guarded on `rev`).
    pub fn move_node(&mut self, id: &str, place: Place, rev: &Rev) -> Result<Cas> {
        let current_hash = self.entry_hash(id)?;
        let current_rev = rev_of(&current_hash);
        if &current_rev != rev {
            let current = render::render_node(&self.read_node(id)?);
            return Ok(Cas::Stale { current_rev, current });
        }
        if let Place::Under(p) = &place {
            // target must exist and must not be the node or a descendant (no cycle)
            let entry = find(&self.spine.root, id).expect("entry exists (checked above)");
            let mut sub = Vec::new();
            collect_ids(entry, &mut sub);
            if sub.iter().any(|s| s == p) {
                return Err(Error::Invalid(format!(
                    "cannot move '{id}' under itself or its own descendant"
                )));
            }
            if find(&self.spine.root, p).is_none() {
                return Err(Error::NotFound(format!("parent '{p}'")));
            }
        }
        let subtree = remove_entry(&mut self.spine.root, id)
            .ok_or_else(|| Error::NotFound(format!("node '{id}'")))?;
        match place {
            Place::Root => self.spine.root.push(subtree),
            Place::Under(p) => {
                find_mut(&mut self.spine.root, &p)
                    .expect("parent exists (checked above)")
                    .children
                    .push(subtree);
            }
        }
        self.flush()?;
        Ok(Cas::Ok(None))
    }

    /// Add a typed link `from` → `to` (idempotent; `to` is a `krg://` reference).
    pub fn set_link(&mut self, from: &str, to: &str, ty: &str) -> Result<()> {
        if find(&self.spine.root, from).is_none() {
            return Err(Error::NotFound(format!("source node '{from}'")));
        }
        let exists = self
            .links
            .iter()
            .any(|l| l.from.0 == from && l.to.0 == to && l.ty == ty);
        if !exists {
            self.links.push(Link {
                from: NodeId(from.to_string()),
                to: Ref(to.to_string()),
                ty: ty.to_string(),
            });
            self.flush()?;
        }
        Ok(())
    }

    /// Remove a typed link (idempotent).
    pub fn remove_link(&mut self, from: &str, to: &str, ty: &str) -> Result<()> {
        let before = self.links.len();
        self.links
            .retain(|l| !(l.from.0 == from && l.to.0 == to && l.ty == ty));
        if self.links.len() != before {
            self.flush()?;
        }
        Ok(())
    }

    /// Add a `media` node. In `embedded` documents `source` is a local file path
    /// (its bytes are copied into `media/`); in `referenced` documents `source`
    /// is a URL/URI stored as `src`.
    pub fn add_media(
        &mut self,
        place: Place,
        media_kind: &str,
        source: &str,
        alt: Option<&str>,
        caption: Option<&str>,
    ) -> Result<(String, Rev)> {
        let mut attrs: Attrs = BTreeMap::new();
        attrs.insert("media_kind".into(), Value::Str(media_kind.to_string()));
        match self.manifest.media_mode {
            MediaMode::Embedded => {
                let bytes = std::fs::read(source).map_err(|e| Error::Io(format!("{source}: {e}")))?;
                let asset = ulid::Ulid::new().to_string();
                let ext = Path::new(source)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("bin");
                self.store
                    .write_part(&format!("media/{asset}.{ext}"), &bytes)?;
                attrs.insert("asset".into(), Value::Str(asset));
            }
            MediaMode::Referenced => {
                attrs.insert("src".into(), Value::Str(source.to_string()));
            }
        }
        if let Some(a) = alt {
            attrs.insert("alt".into(), Value::Str(a.to_string()));
        }
        if let Some(c) = caption {
            attrs.insert("caption".into(), Value::Str(c.to_string()));
        }
        self.insert(place, "media", "", attrs)
    }

    /// Repack the working copy into a packed `.krg`.
    pub fn save(&self, out: impl AsRef<Path>) -> Result<()> {
        container::pack_dir(&self.dir, out.as_ref())
    }

    // --- internals ---------------------------------------------------------

    fn flush(&mut self) -> Result<()> {
        let manifest = to_pretty_value(&self.manifest)?;
        self.store.write_part("manifest.json", &manifest)?;
        let spine = to_pretty_value(&self.spine)?;
        self.store.write_part("spine.json", &spine)?;
        if self.links.is_empty() {
            let _ = self.store.remove_part("links.json");
        } else {
            let links = Links { links: self.links.clone() };
            self.store.write_part("links.json", &to_pretty_value(&links)?)?;
        }
        Ok(())
    }

    fn read_node(&self, id: &str) -> Result<Node> {
        let raw = self.store.read_part(&format!("nodes/{id}.json"))?;
        serde_json::from_slice(&raw).map_err(|e| Error::Parse(format!("nodes/{id}.json: {e}")))
    }

    fn entry_hash(&self, id: &str) -> Result<String> {
        find(&self.spine.root, id)
            .map(|e| e.hash.clone())
            .ok_or_else(|| Error::NotFound(format!("node '{id}'")))
    }
}

// --- helpers ---------------------------------------------------------------

fn build_content(ty: &str, md: &str) -> (NodeContent, BTreeMap<String, MarkDef>) {
    match ty {
        "code" => (NodeContent::Raw(md.to_string()), BTreeMap::new()),
        "heading" | "paragraph" | "table-cell" => {
            let (runs, marks) = render::parse_inline(md);
            (NodeContent::Inline(runs), marks)
        }
        _ => (NodeContent::Empty, BTreeMap::new()),
    }
}

fn plaintext(node: &Node) -> String {
    match &node.content {
        NodeContent::Inline(runs) => runs.iter().map(|r| r.text.as_str()).collect(),
        _ => String::new(),
    }
}

fn to_pretty(node: &Node) -> Result<Vec<u8>> {
    to_pretty_value(node)
}

fn to_pretty_value<T: serde::Serialize>(v: &T) -> Result<Vec<u8>> {
    let mut s = serde_json::to_string_pretty(v).map_err(|e| Error::Parse(e.to_string()))?;
    s.push('\n');
    Ok(s.into_bytes())
}

fn find<'a>(entries: &'a [SpineEntry], id: &str) -> Option<&'a SpineEntry> {
    for e in entries {
        if e.id.0 == id {
            return Some(e);
        }
        if let Some(f) = find(&e.children, id) {
            return Some(f);
        }
    }
    None
}

fn find_mut<'a>(entries: &'a mut [SpineEntry], id: &str) -> Option<&'a mut SpineEntry> {
    for e in entries.iter_mut() {
        if e.id.0 == id {
            return Some(e);
        }
        if let Some(f) = find_mut(&mut e.children, id) {
            return Some(f);
        }
    }
    None
}

fn remove_entry(entries: &mut Vec<SpineEntry>, id: &str) -> Option<SpineEntry> {
    if let Some(pos) = entries.iter().position(|e| e.id.0 == id) {
        return Some(entries.remove(pos));
    }
    for e in entries.iter_mut() {
        if let Some(r) = remove_entry(&mut e.children, id) {
            return Some(r);
        }
    }
    None
}

fn collect_ids(e: &SpineEntry, out: &mut Vec<String>) {
    out.push(e.id.0.clone());
    for c in &e.children {
        collect_ids(c, out);
    }
}
