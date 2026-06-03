//! The schema-driven type system (format §6.2–§6.4).
//!
//! The engine validates, renders, and edits against type *descriptors* rather
//! than a fixed enum of node types.

use std::collections::BTreeMap;

/// A node type's own payload kind (format §6.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentModel {
    Empty,
    Inline,
    Raw,
}

/// Allowed children of a container type (format §6.3).
#[derive(Debug, Clone)]
pub enum ChildRule {
    /// The `"block"` shorthand — any block-level type.
    Block,
    /// An explicit allow-list of child type names.
    Types(Vec<String>),
}

/// Describes a node type's shape (format §6.3).
#[derive(Debug, Clone)]
pub struct TypeDescriptor {
    pub content: ContentModel,
    /// `Some(..)` ⇒ the type is a container.
    pub children: Option<ChildRule>,
    /// Permitted attributes (name → value-domain). Placeholder until the
    /// attr-schema design is fleshed out.
    pub attrs: BTreeMap<String, String>,
}

/// The built-in base schema for v0.1 (`heading`, `paragraph`, … plus the
/// `table` set), format §6.2.
pub fn base_schema() -> BTreeMap<String, TypeDescriptor> {
    unimplemented!("v0.1 base schema descriptors")
}
