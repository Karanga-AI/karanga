//! In-memory document model (format §4–§8), (de)serialized from the JSON parts
//! via `serde`.
//!
//! Serialization mirrors the on-disk shape exactly (empty `attrs`/`x` and
//! absent content are omitted; `ty`→`type`, `ext`→`x`) so that re-serializing
//! a parsed node and hashing it reproduces the stored hash.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::id::{DocId, NodeId, Ref};
use crate::schema::TypeDescriptor;

/// A value in the no-float domain (format §9.1). The deliberate absence of a
/// floating-point variant is how the type system enforces "integers only":
/// a fractional JSON number fails to deserialize into any variant.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Str(String),
    List(Vec<Value>),
    Map(BTreeMap<String, Value>),
}

pub type Attrs = BTreeMap<String, Value>;

/// One atomic node — a `nodes/<id>.json` part.
///
/// `content` is a single string or absent; its *interpretation* comes from the
/// type's content model (format §6.3): canonical Karanga Markdown inline
/// syntax for `inline` types (§7), an opaque string for `raw` types.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Node {
    pub id: NodeId,
    #[serde(rename = "type")]
    pub ty: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "map_is_empty")]
    pub attrs: Attrs,
    #[serde(default, rename = "x", skip_serializing_if = "map_is_empty")]
    pub ext: Attrs,
}

/// Document metadata (`manifest.json`, format §4).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Manifest {
    pub krg: String,
    pub doc_id: DocId,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
    pub media_mode: MediaMode,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<Author>,
    /// Descriptors for non-base node types this document uses (format §6.4).
    #[serde(default, skip_serializing_if = "map_is_empty")]
    pub types: BTreeMap<String, TypeDescriptor>,
    #[serde(default, rename = "x", skip_serializing_if = "map_is_empty")]
    pub ext: Attrs,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaMode {
    Embedded,
    Referenced,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Author {
    pub name: String,
    #[serde(default, rename = "x", skip_serializing_if = "map_is_empty")]
    pub ext: Attrs,
}

/// The ordered tree + index projection (`spine.json`, format §5).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Spine {
    pub root: Vec<SpineEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SpineEntry {
    pub id: NodeId,
    #[serde(rename = "type")]
    pub ty: String,
    pub hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<SpineEntry>,
}

/// A typed, directed link (`links.json`, format §8.3).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Link {
    pub from: NodeId,
    pub to: Ref,
    #[serde(rename = "type")]
    pub ty: String,
}

/// Wrapper matching `links.json`'s `{ "links": [...] }` shape.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Links {
    pub links: Vec<Link>,
}

// --- serde skip helpers ----------------------------------------------------

fn map_is_empty<K, V>(m: &BTreeMap<K, V>) -> bool {
    m.is_empty()
}
