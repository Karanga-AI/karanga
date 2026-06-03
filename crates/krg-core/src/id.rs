//! Identifiers and `krg://` references (format §3).

use serde::{Deserialize, Serialize};

use crate::Result;

/// A document identifier — a UUID.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(transparent)]
pub struct DocId(pub String);

/// A node identifier — opaque, unique within its document
/// (`^[A-Za-z0-9_-]{1,64}$`; ULID recommended).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(transparent)]
pub struct NodeId(pub String);

/// A `krg://` reference to a node or a document (an opaque handle to callers).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(transparent)]
pub struct Ref(pub String);

/// A node revision token: the first 12 hex of the content hash (interface §2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rev(pub String);

impl Ref {
    /// `krg://<doc_id>/<node_id>`
    pub fn node(doc: &DocId, node: &NodeId) -> Ref {
        Ref(format!("krg://{}/{}", doc.0, node.0))
    }
    /// `krg://<doc_id>/`
    pub fn document(doc: &DocId) -> Ref {
        Ref(format!("krg://{}/", doc.0))
    }
    /// Parse and validate a `krg://` reference (format §3.1).
    pub fn parse(s: &str) -> Result<Ref> {
        unimplemented!("krg:// parsing")
    }
}
