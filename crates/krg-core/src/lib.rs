//! `krg-core` — the Karanga document engine.
//!
//! Implements the `.krg` file format (`spec/format-v0.1.md`) and the operation
//! interface (`spec/interface-v0.1.md`). Architecture: `docs/core-architecture-v0.1.md`.
//!
//! This crate is currently a **scaffold**: the module structure and public
//! surface are in place; bodies are `unimplemented!()` pending implementation.
#![allow(dead_code, unused_variables, unused_imports)]

pub mod container;
pub mod document;
pub mod edit;
pub mod error;
pub mod hash;
pub mod id;
pub mod model;
pub mod query;
pub mod render;
pub mod schema;
pub mod scope;
pub mod validate;

pub use error::{Error, Result};
pub use id::{DocId, NodeId, Ref, Rev};
pub use model::{Node, NodeContent, Run};

/// The format spec version this build targets.
pub const FORMAT_VERSION: &str = "0.1";
/// The operation-interface version this build targets.
pub const INTERFACE_VERSION: &str = "0.1";
