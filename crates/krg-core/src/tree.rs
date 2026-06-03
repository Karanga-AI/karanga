//! The editor document tree — the serializable contract a WYSIWYG client
//! exchanges with the engine.
//!
//! `Document::to_tree` produces it (engine → editor); `Workspace::set_tree`
//! reconciles it back (editor → engine), **preserving the `id` of any block
//! the editor kept** so links, CAS tokens, and atomic addressing survive an
//! edit. Blocks without an `id` are new and get a fresh `node_id`.

use serde::{Deserialize, Serialize};

use crate::model::Attrs;

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
