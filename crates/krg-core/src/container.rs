//! ZIP container + exploded-dir I/O and the `Store` abstraction
//! (core-architecture §4). Hides the packed/exploded duality.

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::error::Error;
use crate::Result;

/// The fixed `mimetype` content (format §2).
pub const MIMETYPE: &str = "application/vnd.karanga.document+zip";

/// Abstracts a document's parts, whether packed (`.krg`) or exploded (a dir),
/// so `query`/`render`/`edit` work against either.
pub trait Store {
    fn read_part(&self, path: &str) -> Result<Vec<u8>>;
    fn list(&self, prefix: &str) -> Result<Vec<String>>;
    fn write_part(&mut self, path: &str, bytes: &[u8]) -> Result<()>;
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

    /// Atomic: write to a sibling temp file, then rename.
    fn write_part(&mut self, path: &str, bytes: &[u8]) -> Result<()> {
        let full = self.root.join(path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::Io(e.to_string()))?;
        }
        let tmp = full.with_extension("krgtmp");
        std::fs::write(&tmp, bytes).map_err(|e| Error::Io(format!("{path}: {e}")))?;
        std::fs::rename(&tmp, &full).map_err(|e| Error::Io(format!("{path}: {e}")))?;
        Ok(())
    }

    fn remove_part(&mut self, path: &str) -> Result<()> {
        std::fs::remove_file(self.root.join(path)).map_err(|e| Error::Io(format!("{path}: {e}")))
    }
}

/// Explode a packed `.krg` into a working directory.
pub fn explode(krg: &Path, dir: &Path) -> Result<()> {
    let file = File::open(krg).map_err(|e| Error::Io(e.to_string()))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| Error::Parse(format!("zip: {e}")))?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| Error::Parse(format!("zip: {e}")))?;
        let name = entry.name().to_string();
        if name.ends_with('/') {
            continue;
        }
        let out = dir.join(&name);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::Io(e.to_string()))?;
        }
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf).map_err(|e| Error::Io(e.to_string()))?;
        std::fs::write(&out, buf).map_err(|e| Error::Io(e.to_string()))?;
    }
    Ok(())
}

/// Repack a working directory into a `.krg` (format §2): `mimetype` first and
/// `STORE`d, `manifest.json` `STORE`d, everything else `DEFLATE`d.
pub fn pack_dir(dir: &Path, out: &Path) -> Result<()> {
    let file = File::create(out).map_err(|e| Error::Io(e.to_string()))?;
    let mut zip = zip::write::ZipWriter::new(file);
    let stored = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored);
    let deflated = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    let zerr = |e: zip::result::ZipError| Error::Io(format!("zip: {e}"));
    let ioerr = |e: std::io::Error| Error::Io(e.to_string());

    zip.start_file("mimetype", stored).map_err(zerr)?;
    zip.write_all(MIMETYPE.as_bytes()).map_err(ioerr)?;

    zip.start_file("manifest.json", stored).map_err(zerr)?;
    zip.write_all(&std::fs::read(dir.join("manifest.json")).map_err(ioerr)?)
        .map_err(ioerr)?;

    let mut rels = Vec::new();
    collect_rel(dir, dir, &mut rels)?;
    rels.sort();
    for rel in rels {
        if rel == "mimetype" || rel == "manifest.json" || rel.ends_with(".krgtmp") {
            continue;
        }
        zip.start_file(rel.clone(), deflated).map_err(zerr)?;
        zip.write_all(&std::fs::read(dir.join(&rel)).map_err(ioerr)?)
            .map_err(ioerr)?;
    }
    zip.finish().map_err(zerr)?;
    Ok(())
}

fn collect_rel(base: &Path, cur: &Path, out: &mut Vec<String>) -> Result<()> {
    for e in std::fs::read_dir(cur)
        .map_err(|e| Error::Io(e.to_string()))?
        .flatten()
    {
        let p = e.path();
        if p.is_dir() {
            collect_rel(base, &p, out)?;
        } else if let Ok(rel) = p.strip_prefix(base) {
            if let Some(s) = rel.to_str() {
                out.push(s.replace('\\', "/"));
            }
        }
    }
    Ok(())
}
