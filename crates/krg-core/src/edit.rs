//! Write verbs with optimistic CAS (interface §4–§5).
//!
//! `rev` is required for mutations of an existing node (`update`/`move`/
//! `delete`); creation ops and idempotent link ops need none. There is no
//! last-writer-wins path.

use crate::id::{Ref, Rev};
use crate::Result;

/// Result of a write. `Stale` is a normal outcome (interface §5), not an error.
#[derive(Debug)]
pub enum WriteOut {
    Created(Ref),
    Updated { r: Ref, rev: Rev },
    Ok,
    /// CAS conflict: the on-disk node changed since `rev` was read.
    Stale { current_rev: Rev, current: String },
}

/// Where to place a node, relative to a parent and a sibling/index.
#[derive(Debug)]
pub enum Position {
    After { parent: Option<Ref>, after: Ref },
    Index { parent: Option<Ref>, index: usize },
}

pub fn create_document(title: &str, description: Option<&str>) -> Result<Ref> {
    unimplemented!("create_document")
}

/// `content` is Karanga Markdown; the engine structures it into the node model.
pub fn insert_node(doc: &Ref, ty: &str, content: &str, at: Position) -> Result<WriteOut> {
    unimplemented!("insert_node")
}

pub fn update_node(node: &Ref, content: Option<&str>, rev: &Rev) -> Result<WriteOut> {
    unimplemented!("update_node")
}

pub fn move_node(node: &Ref, to: Position, rev: Option<&Rev>) -> Result<WriteOut> {
    unimplemented!("move_node")
}

pub fn delete_node(node: &Ref, rev: &Rev) -> Result<WriteOut> {
    unimplemented!("delete_node")
}

pub fn set_link(from: &Ref, to: &Ref, ty: &str) -> Result<WriteOut> {
    unimplemented!("set_link")
}

pub fn remove_link(from: &Ref, to: &Ref, ty: &str) -> Result<WriteOut> {
    unimplemented!("remove_link")
}

pub fn add_media(doc: &Ref, media_kind: &str, source: &str, at: Position) -> Result<WriteOut> {
    unimplemented!("add_media")
}
