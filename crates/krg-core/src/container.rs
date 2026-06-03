//! ZIP container + exploded-dir I/O and the `Store` abstraction
//! (core-architecture §4). Hides the packed/exploded duality.

use std::path::PathBuf;

use crate::Result;

/// Abstracts a document's parts, whether packed (`.krg`) or exploded (a dir),
/// so `query`/`render`/`edit` work against either.
pub trait Store {
    /// Read one part by path (e.g. `nodes/<id>.json`) — random-access.
    fn read_part(&self, path: &str) -> Result<Vec<u8>>;
    /// List part paths under a prefix.
    fn list(&self, prefix: &str) -> Result<Vec<String>>;
    /// Write one part atomically.
    fn write_part(&mut self, path: &str, bytes: &[u8]) -> Result<()>;
    /// Remove one part.
    fn remove_part(&mut self, path: &str) -> Result<()>;
}

/// Read-optimized view over a packed `.krg`. Uses the ZIP central directory to
/// extract a single entry without inflating the whole archive.
pub struct ZipStore {
    path: PathBuf,
}

/// Read/write view over an exploded working directory (the form edits run on).
pub struct DirStore {
    root: PathBuf,
}
