//! Read verbs (interface §3). All return projections, never raw JSON.

use crate::id::{Ref, Rev};
use crate::model::Link;
use crate::scope::Scope;
use crate::Result;

/// Tier-1 discovery result.
#[derive(Debug)]
pub struct DocHit {
    pub r: Ref,
    pub title: String,
    pub description: Option<String>,
}

/// A node listed by `find_nodes`.
#[derive(Debug)]
pub struct NodeHit {
    pub r: Ref,
    pub ty: String,
    pub label: Option<String>,
}

/// A full-text/fuzzy search hit.
#[derive(Debug)]
pub struct SearchHit {
    pub doc: Ref,
    pub node: Ref,
    pub snippet: String,
}

#[derive(Debug, Clone, Copy)]
pub enum Direction {
    Out,
    In,
    Both,
}

/// Tier 1 — discovery (manifest-level; no node bodies read).
pub fn find_documents(query: &str, scope: &Scope, limit: usize) -> Result<Vec<DocHit>> {
    unimplemented!("find_documents")
}

/// Tier 2 — the outline (headings only; from `spine.json`).
pub fn get_outline(doc: &Ref) -> Result<String> {
    unimplemented!("get_outline")
}

/// Tier 3 — one rendered node, plus its `rev` for a follow-up CAS write.
pub fn get_node(node: &Ref) -> Result<RenderedNode> {
    unimplemented!("get_node")
}

#[derive(Debug)]
pub struct RenderedNode {
    pub r: Ref,
    pub ty: String,
    pub rev: Rev,
    pub content: String,
}

/// A rendered section subtree.
pub fn get_section(heading: &Ref) -> Result<String> {
    unimplemented!("get_section")
}

/// Filter nodes by segment type (from `spine.json`; no body reads).
pub fn find_nodes(doc: &Ref, ty: Option<&str>) -> Result<Vec<NodeHit>> {
    unimplemented!("find_nodes")
}

/// Full-text / fuzzy search across `scope`.
pub fn search(query: &str, scope: &Scope) -> Result<Vec<SearchHit>> {
    unimplemented!("search")
}

/// Traverse the link graph from a node.
pub fn get_links(node: &Ref, dir: Direction, scope: &Scope) -> Result<Vec<Link>> {
    unimplemented!("get_links")
}
