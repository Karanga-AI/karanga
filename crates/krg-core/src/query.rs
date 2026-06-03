//! Cross-document read verbs (interface §3, §6). Single-document reads live on
//! [`crate::document::Document`]; the functions here span a directory `scope`.

use crate::container::{Store, ZipStore};
use crate::id::Ref;
use crate::model::{Link, Links, Manifest};
use crate::scope::Scope;
use crate::Result;

/// Tier-1 discovery result.
#[derive(Debug)]
pub struct DocHit {
    pub r: Ref,
    pub title: String,
    pub description: Option<String>,
}

/// A full-text search hit.
#[derive(Debug)]
pub struct SearchHit {
    pub doc: Ref,
    pub node: Ref,
    pub snippet: String,
}

/// Direction of link traversal (used by `Document::get_links` and `backlinks`).
#[derive(Debug, Clone, Copy)]
pub enum Direction {
    Out,
    In,
    Both,
}

/// Tier 1 — discovery (manifest-level only; no node bodies read).
pub fn find_documents(query: &str, scope: &Scope, limit: usize) -> Result<Vec<DocHit>> {
    let needle = query.to_lowercase();
    let mut hits = Vec::new();
    for path in scope.documents()? {
        let store = ZipStore::open(&path);
        let manifest: Manifest = match store.read_part("manifest.json") {
            Ok(bytes) => match serde_json::from_slice(&bytes) {
                Ok(m) => m,
                Err(_) => continue,
            },
            Err(_) => continue,
        };
        let hay = format!(
            "{} {}",
            manifest.title,
            manifest.description.clone().unwrap_or_default()
        )
        .to_lowercase();
        if needle.is_empty() || hay.contains(&needle) {
            hits.push(DocHit {
                r: Ref::document(&manifest.doc_id),
                title: manifest.title,
                description: manifest.description,
            });
            if hits.len() >= limit {
                break;
            }
        }
    }
    Ok(hits)
}

/// Cross-document backlinks: every link in `scope` whose target is `target_ref`
/// (a full `krg://<doc>/<node>` reference). A same-document link recorded with
/// the short `krg:///<node>` form is matched too.
pub fn backlinks(target_ref: &str, scope: &Scope) -> Result<Vec<Link>> {
    let (target_doc, target_node) = split_ref(target_ref);
    let short = target_node.as_ref().map(|n| format!("krg:///{n}"));
    let mut out = Vec::new();
    for path in scope.documents()? {
        let store = ZipStore::open(&path);
        let manifest: Manifest = match store.read_part("manifest.json") {
            Ok(b) => match serde_json::from_slice(&b) {
                Ok(m) => m,
                Err(_) => continue,
            },
            Err(_) => continue,
        };
        let links: Vec<Link> = match store.read_part("links.json") {
            Ok(b) => serde_json::from_slice::<Links>(&b).map(|l| l.links).unwrap_or_default(),
            Err(_) => continue,
        };
        let same_doc = target_doc.as_deref() == Some(manifest.doc_id.0.as_str());
        for l in links {
            let hit = l.to.0 == target_ref
                || (same_doc && short.as_deref() == Some(l.to.0.as_str()));
            if hit {
                out.push(l);
            }
        }
    }
    Ok(out)
}

/// Split `krg://<doc>/<node>` (or `krg:///<node>`) into its parts.
fn split_ref(r: &str) -> (Option<String>, Option<String>) {
    let rest = r.strip_prefix("krg://").unwrap_or(r);
    let mut it = rest.splitn(2, '/');
    let doc = it.next().filter(|s| !s.is_empty()).map(String::from);
    let node = it.next().filter(|s| !s.is_empty()).map(String::from);
    (doc, node)
}

/// Full-text search across `scope` (backed by Tantivy when the `search`
/// feature is enabled — the default).
#[cfg(feature = "search")]
pub fn search(query: &str, scope: &Scope) -> Result<Vec<SearchHit>> {
    crate::search::search(query, scope)
}

#[cfg(not(feature = "search"))]
pub fn search(_query: &str, _scope: &Scope) -> Result<Vec<SearchHit>> {
    Err(crate::error::Error::Unsupported(
        "built without the `search` feature".into(),
    ))
}

/// Force a rebuild of the persistent search index for `scope`; returns the node
/// count indexed.
#[cfg(feature = "search")]
pub fn reindex(scope: &Scope) -> Result<usize> {
    crate::search::reindex(scope)
}

#[cfg(not(feature = "search"))]
pub fn reindex(_scope: &Scope) -> Result<usize> {
    Err(crate::error::Error::Unsupported(
        "built without the `search` feature".into(),
    ))
}
