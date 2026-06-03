//! The schema-driven type system (format §6.2–§6.4).
//!
//! The engine validates, renders, and edits against type *descriptors* rather
//! than a fixed enum of node types.

use std::collections::BTreeMap;
use std::fmt;

use serde::de::{self, Deserializer, SeqAccess, Visitor};
use serde::Deserialize;

/// A node type's own payload kind (format §6.3).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContentModel {
    Empty,
    Inline,
    Raw,
}

/// Allowed children of a container type (format §6.3): the `"block"` shorthand
/// or an explicit allow-list of type names.
#[derive(Debug, Clone)]
pub enum ChildRule {
    Block,
    Types(Vec<String>),
}

impl<'de> Deserialize<'de> for ChildRule {
    fn deserialize<D>(d: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = ChildRule;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("\"block\" or an array of type names")
            }
            fn visit_str<E: de::Error>(self, s: &str) -> std::result::Result<ChildRule, E> {
                if s == "block" {
                    Ok(ChildRule::Block)
                } else {
                    Err(E::custom(format!("unknown child shorthand '{s}'")))
                }
            }
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                seq: A,
            ) -> std::result::Result<ChildRule, A::Error> {
                let v = Vec::<String>::deserialize(de::value::SeqAccessDeserializer::new(seq))?;
                Ok(ChildRule::Types(v))
            }
        }
        d.deserialize_any(V)
    }
}

/// Describes a node type's shape (format §6.3).
#[derive(Debug, Clone, Deserialize)]
pub struct TypeDescriptor {
    pub content: ContentModel,
    /// `Some(..)` ⇒ the type is a container.
    #[serde(default)]
    pub children: Option<ChildRule>,
    /// Permitted attributes (name → value-domain). Placeholder representation.
    #[serde(default)]
    pub attrs: BTreeMap<String, String>,
    // `render` hints are advisory and ignored here (unknown fields are dropped).
}

/// The built-in base schema for v0.1 (`heading`, `paragraph`, … plus the
/// `table` set), format §6.2.
pub fn base_schema() -> BTreeMap<String, TypeDescriptor> {
    unimplemented!("v0.1 base schema descriptors")
}
