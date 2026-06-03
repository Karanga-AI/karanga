//! ZIP container + exploded-dir I/O and the `Store` abstraction
//! (core-architecture §4). Hides the packed/exploded duality.

use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

use crate::error::Error;
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

/// Read-only view over a packed `.krg`. Opens the archive per read and uses the
/// ZIP central directory to extract a single entry without inflating the rest.
pub struct ZipStore {
    path: PathBuf,
}

impl ZipStore {
    pub fn open(path: impl Into<PathBuf>) -> ZipStore {
        ZipStore { path: path.into() }
    }

    fn archive(&self) -> Result<zip::ZipArchive<File>> {
        let file = File::open(&self.path).map_err(|e| Error::Io(e.to_string()))?;
        zip::ZipArchive::new(file).map_err(|e| Error::Parse(format!("zip: {e}")))
    }
}

impl Store for ZipStore {
    fn read_part(&self, path: &str) -> Result<Vec<u8>> {
        let mut archive = self.archive()?;
        let mut entry = archive
            .by_name(path)
            .map_err(|_| Error::NotFound(path.to_string()))?;
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf).map_err(|e| Error::Io(e.to_string()))?;
        Ok(buf)
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let mut archive = self.archive()?;
        let mut out = Vec::new();
        for i in 0..archive.len() {
            let f = archive.by_index(i).map_err(|e| Error::Parse(e.to_string()))?;
            let name = f.name().to_string();
            if name.starts_with(prefix) {
                out.push(name);
            }
        }
        Ok(out)
    }

    fn write_part(&mut self, _path: &str, _bytes: &[u8]) -> Result<()> {
        Err(Error::Unsupported("ZipStore is read-only; edit the exploded form".into()))
    }
    fn remove_part(&mut self, _path: &str) -> Result<()> {
        Err(Error::Unsupported("ZipStore is read-only".into()))
    }
}

/// Read/write view over an exploded working directory (the form edits run on).
pub struct DirStore {
    root: PathBuf,
}

impl DirStore {
    pub fn open(root: impl Into<PathBuf>) -> DirStore {
        DirStore { root: root.into() }
    }
}

impl Store for DirStore {
    fn read_part(&self, path: &str) -> Result<Vec<u8>> {
        std::fs::read(self.root.join(path)).map_err(|e| Error::Io(format!("{path}: {e}")))
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let dir = self.root.join(prefix);
        let mut out = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                if let Some(name) = e.file_name().to_str() {
                    out.push(format!("{}/{}", prefix.trim_end_matches('/'), name));
                }
            }
        }
        Ok(out)
    }

    fn write_part(&mut self, _path: &str, _bytes: &[u8]) -> Result<()> {
        Err(Error::Unsupported("DirStore write lands in the write slice".into()))
    }
    fn remove_part(&mut self, _path: &str) -> Result<()> {
        Err(Error::Unsupported("DirStore remove lands in the write slice".into()))
    }
}
