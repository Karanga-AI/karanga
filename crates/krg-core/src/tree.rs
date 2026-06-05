//! The editor document tree — the serializable contract a WYSIWYG client
//! exchanges with the engine.
//!
//! `Document::to_tree` produces it (engine → editor); `Workspace::set_tree`
//! reconciles it back (editor → engine), **preserving the `id` of any block
//! the editor kept** so links, CAS tokens, and atomic addressing survive an
//! edit. Blocks without an `id` are new and get a fresh `node_id`.

use serde::{Deserialize, Serialize};

use crate::hash::content_hash;
use crate::id::NodeId;
use crate::model::{Attrs, Node};
use crate::render;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorBlock {
    /// Existing node id (kept by the editor) or `None` for a newly typed block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub ty: String,
    /// Inline content as Karanga Markdown for text-bearing types, raw text for
    /// `code`, empty for containers/divider/media.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub content: String,
    #[serde(default)]
    pub attrs: Attrs,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<EditorBlock>,
}

/// A node as it would be configured in the `.krg`, for the editor's debug
/// inspector. `path` is positional (`"0"`, `"0.1"`) and stable across keystrokes
/// (real ULIDs are assigned only on save); `node` is the raw node JSON.
#[derive(Debug, Clone, Serialize)]
pub struct InspectNode {
    pub path: String,
    #[serde(rename = "type")]
    pub ty: String,
    pub hash: String,
    pub node: serde_json::Value,
    pub children: Vec<InspectNode>,
}

/// Report the node configuration that the given Karanga Markdown would produce.
pub fn inspect_markdown(md: &str) -> Vec<InspectNode> {
    build_inspect(render::parse_markdown(md), "")
}

fn build_inspect(blocks: Vec<render::Block>, prefix: &str) -> Vec<InspectNode> {
    blocks
        .into_iter()
        .enumerate()
        .map(|(i, b)| {
            let path = if prefix.is_empty() {
                i.to_string()
            } else {
                format!("{prefix}.{i}")
            };
            let render::Block { ty, content, attrs, children } = b;
            let node = Node {
                id: NodeId(path.clone()),
                ty: ty.clone(),
                content,
                attrs,
                ext: Attrs::new(),
            };
            let hash = content_hash(&node).unwrap_or_default();
            let value = serde_json::to_value(&node).unwrap_or(serde_json::Value::Null);
            let kids = build_inspect(children, &path);
            InspectNode { path, ty, hash, node: value, children: kids }
        })
        .collect()
}
