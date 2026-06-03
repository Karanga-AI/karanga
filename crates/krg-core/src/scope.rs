//! Cross-document scope (interface §6): a plain directory path. There is no
//! "vault"/registry/marker — the filesystem is the collection.

use std::path::{Path, PathBuf};

use crate::error::Error;
use crate::Result;

/// A file or a directory (searched recursively) that bounds cross-document ops.
pub struct Scope(pub PathBuf);

impl Scope {
    pub fn new(path: impl Into<PathBuf>) -> Scope {
        Scope(path.into())
    }

    /// All `.krg` files within the scope (recursive), sorted.
    pub fn documents(&self) -> Result<Vec<PathBuf>> {
        let mut out = Vec::new();
        collect(&self.0, &mut out)?;
        out.sort();
        Ok(out)
    }
}

fn collect(p: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    if p.is_file() {
        if p.extension().and_then(|e| e.to_str()) == Some("krg") {
            out.push(p.to_path_buf());
        }
        return Ok(());
    }
    if p.is_dir() {
        for e in std::fs::read_dir(p).map_err(|e| Error::Io(e.to_string()))?.flatten() {
            collect(&e.path(), out)?;
        }
    }
    Ok(())
}
