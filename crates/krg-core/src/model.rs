//! In-memory document model (format §4–§8), deserialized from the JSON parts
//! via `serde`.

use std::collections::BTreeMap;
use std::fmt;

use serde::de::{self, Deserializer, SeqAccess, Visitor};
use serde::Deserialize;

use crate::id::{DocId, NodeId, Ref};
use crate::schema::TypeDescriptor;

/// A value in the no-float domain (format §9.1). The deliberate absence of a
/// floating-point variant is how the type system enforces "integers only":
/// a fractional JSON number fails to deserialize into any variant.
#[derive(Debug, Clone, PartialEq, Deserialize)]
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
#[derive(Debug, Clone, Deserialize)]
pub struct Node {
    pub id: NodeId,
    #[serde(rename = "type")]
    pub ty: String,
    #[serde(default)]
    pub content: NodeContent,
    #[serde(default)]
    pub attrs: Attrs,
    #[serde(default)]
    pub marks: BTreeMap<String, MarkDef>,
    #[serde(default, rename = "x")]
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

/// A run of text carrying zero or more marks (format §7).
#[derive(Debug, Clone, Deserialize)]
pub struct Run {
    pub text: String,
    #[serde(default)]
    pub marks: Vec<String>,
}

/// A parametric mark definition (format §7.2).
#[derive(Debug, Clone, Deserialize)]
pub struct MarkDef {
    #[serde(rename = "type")]
    pub ty: String,
    #[serde(default)]
    pub href: Option<String>,
    #[serde(default)]
    pub target: Option<Ref>,
}

/// Document metadata (`manifest.json`, format §4).
#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub krg: String,
    pub doc_id: DocId,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub created: Option<String>,
    #[serde(default)]
    pub modified: Option<String>,
    pub media_mode: MediaMode,
    #[serde(default)]
    pub authors: Vec<Author>,
    /// Descriptors for non-base node types this document uses (format §6.4).
    #[serde(default)]
    pub types: BTreeMap<String, TypeDescriptor>,
    #[serde(default, rename = "x")]
    pub ext: Attrs,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaMode {
    Embedded,
    Referenced,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Author {
    pub name: String,
    #[serde(default, rename = "x")]
    pub ext: Attrs,
}

/// The ordered tree + index projection (`spine.json`, format §5).
#[derive(Debug, Clone, Deserialize)]
pub struct Spine {
    pub root: Vec<SpineEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SpineEntry {
    pub id: NodeId,
    #[serde(rename = "type")]
    pub ty: String,
    pub hash: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub children: Vec<SpineEntry>,
}

/// A typed, directed link (`links.json`, format §8.3).
#[derive(Debug, Clone, Deserialize)]
pub struct Link {
    pub from: NodeId,
    pub to: Ref,
    #[serde(rename = "type")]
    pub ty: String,
}

/// Wrapper matching `links.json`'s `{ "links": [...] }` shape.
#[derive(Debug, Clone, Deserialize)]
pub struct Links {
    pub links: Vec<Link>,
}
