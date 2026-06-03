//! Schema-driven projection (read side) and authoring parse (write side).
//!
//! Read: nodes/sections/documents → Karanga Markdown (interface §3, §8); rich
//! for known types, generic-by-content-model for declared custom types.
//! Write: Karanga Markdown → model.

use crate::model::Node;
use crate::Result;

/// Render a single node to Karanga Markdown.
pub fn render_node(node: &Node) -> String {
    unimplemented!("node render")
}

/// Render a whole document (pre-order DFS of the spine).
pub fn render_document() -> Result<String> {
    unimplemented!("document render")
}

/// Parse Karanga Markdown into one or more nodes (authoring inverse).
pub fn parse_markdown(md: &str) -> Result<Vec<Node>> {
    unimplemented!("Karanga Markdown parse")
}
