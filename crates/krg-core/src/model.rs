//! In-memory document model (format §4–§8).

use std::collections::BTreeMap;

use crate::id::{DocId, NodeId, Ref};
use crate::schema::TypeDescriptor;

/// A value in the no-float domain (format §9.1). The deliberate absence of a
/// floating-point variant is how the type system enforces "integers only".
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Str(String),
    Bool(bool),
    List(Vec<Value>),
    Map(BTreeMap<String, Value>),
}

pub type Attrs = BTreeMap<String, Value>;

/// One atomic node — a `nodes/<id>.json` part.
#[derive(Debug, Clone)]
pub struct Node {
    pub id: NodeId,
    /// Base type (bare name) or custom type (`vendor:name`).
    pub ty: String,
    pub content: NodeContent,
    pub attrs: Attrs,
    /// Node-local parametric mark definitions (format §7.2).
    pub marks: BTreeMap<String, MarkDef>,
    /// The `x` extension bag, preserved verbatim.
    pub ext: Attrs,
}

/// A node's own payload, per its type's content model.
#[derive(Debug, Clone)]
pub enum NodeContent {
    Empty,
    Inline(Vec<Run>),
    Raw(String),
}

/// A run of text carrying zero or more marks (format §7).
#[derive(Debug, Clone)]
pub struct Run {
    pub text: String,
    /// Simple keywords (`strong`/`em`/`code`/`strike`) or keys into `Node::marks`.
    pub marks: Vec<String>,
}

/// A parametric mark definition (format §7.2).
#[derive(Debug, Clone)]
pub struct MarkDef {
    pub ty: String,
    pub href: Option<String>,
    pub target: Option<Ref>,
}

/// Document metadata (`manifest.json`, format §4).
#[derive(Debug, Clone)]
pub struct Manifest {
    pub krg: String,
    pub doc_id: DocId,
    pub title: String,
    pub description: Option<String>,
    pub created: Option<String>,
    pub modified: Option<String>,
    pub media_mode: MediaMode,
    pub authors: Vec<Author>,
    /// Descriptors for non-base node types this document uses (format §6.4).
    pub types: BTreeMap<String, TypeDescriptor>,
    pub ext: Attrs,
}

#[derive(Debug, Clone)]
pub enum MediaMode {
    Embedded,
    Referenced,
}

#[derive(Debug, Clone)]
pub struct Author {
    pub name: String,
    pub ext: Attrs,
}

/// The ordered tree + index projection (`spine.json`, format §5).
#[derive(Debug, Clone)]
pub struct Spine {
    pub root: Vec<SpineEntry>,
}

#[derive(Debug, Clone)]
pub struct SpineEntry {
    pub id: NodeId,
    pub ty: String,
    pub hash: String,
    pub label: Option<String>,
    pub children: Vec<SpineEntry>,
}

/// A typed, directed link (`links.json`, format §8.3).
#[derive(Debug, Clone)]
pub struct Link {
    pub from: NodeId,
    pub to: Ref,
    pub ty: String,
}
