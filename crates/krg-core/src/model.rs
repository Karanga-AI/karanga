//! In-memory document model (format §4–§8), (de)serialized from the JSON parts
//! via `serde`.
//!
//! Serialization mirrors the on-disk shape exactly (empty `attrs`/`marks`/`x`
//! and `Empty` content are omitted; `ty`→`type`, `ext`→`x`) so that
//! re-serializing a parsed node and hashing it reproduces the stored hash.

use std::collections::BTreeMap;
use std::fmt;

use serde::de::{self, Deserializer, SeqAccess, Visitor};
use serde::{Deserialize, Serialize, Serializer};

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
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Node {
    pub id: NodeId,
    #[serde(rename = "type")]
    pub ty: String,
    #[serde(default, skip_serializing_if = "content_is_empty")]
    pub content: NodeContent,
    #[serde(default, skip_serializing_if = "map_is_empty")]
    pub attrs: Attrs,
    #[serde(default, skip_serializing_if = "map_is_empty")]
    pub marks: BTreeMap<String, MarkDef>,
    #[serde(default, rename = "x", skip_serializing_if = "map_is_empty")]
    pub ext: Attrs,
}

/// A node's own payload, per its type's content model.
#[derive(Debug, Clone, Default)]
pub enum NodeContent {
    #[default]
    Empty,
    Inline(Vec<Run>),
    Raw(String),
}

// `content` is an array of runs, a raw string, or absent.
impl<'de> Deserialize<'de> for NodeContent {
    fn deserialize<D>(d: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = NodeContent;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("inline runs or a raw string")
            }
            fn visit_str<E: de::Error>(self, s: &str) -> std::result::Result<NodeContent, E> {
                Ok(NodeContent::Raw(s.to_string()))
            }
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                seq: A,
            ) -> std::result::Result<NodeContent, A::Error> {
                let runs = Vec::<Run>::deserialize(de::value::SeqAccessDeserializer::new(seq))?;
                Ok(NodeContent::Inline(runs))
            }
        }
        d.deserialize_any(V)
    }
}

impl Serialize for NodeContent {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        match self {
            NodeContent::Empty => s.serialize_none(), // skipped in practice
            NodeContent::Inline(runs) => runs.serialize(s),
            NodeContent::Raw(t) => s.serialize_str(t),
        }
    }
}

/// A run of text carrying zero or more marks (format §7).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Run {
    pub text: String,
    #[serde(default, skip_serializing_if = "vec_is_empty")]
    pub marks: Vec<String>,
}

/// A parametric mark definition (format §7.2).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MarkDef {
    #[serde(rename = "type")]
    pub ty: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub href: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<Ref>,
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

fn content_is_empty(c: &NodeContent) -> bool {
    matches!(c, NodeContent::Empty)
}
fn map_is_empty<K, V>(m: &BTreeMap<K, V>) -> bool {
    m.is_empty()
}
fn vec_is_empty<T>(v: &Vec<T>) -> bool {
    v.is_empty()
}
