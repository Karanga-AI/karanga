//! Cross-document scope (interface §6): a plain directory path. There is no
//! "vault"/registry/marker — the filesystem is the collection.

use std::path::PathBuf;

/// A file or a directory (searched recursively) that bounds cross-document ops.
pub struct Scope(pub PathBuf);

impl Scope {
    pub fn new(path: impl Into<PathBuf>) -> Scope {
        Scope(path.into())
    }
}
