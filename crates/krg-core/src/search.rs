//! Full-text search over a directory of `.krg` files, backed by a **persistent
//! Tantivy index** in a cache directory (interface §6; core-architecture §7).
//!
//! The index is a rebuildable, path-scoped accelerator — never authoritative,
//! never a format artifact, and stored outside the `.krg` files (in the OS
//! cache, keyed by the scope path). A **fingerprint** (each document's
//! path + mtime + size) lets a search reuse the existing index when the corpus
//! is unchanged and rebuild it only when something changed.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use sha2::{Digest, Sha256};
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{Field, Schema, Value, STORED, TEXT};
use tantivy::{doc, Index, TantivyDocument};

use crate::document::Document;
use crate::error::Error;
use crate::id::{NodeId, Ref};
use crate::query::SearchHit;
use crate::scope::Scope;
use crate::Result;

/// One document's contribution to the corpus fingerprint.
type Fingerprint = Vec<(String, u64, u64)>; // (path, mtime_nanos, len)

pub fn search(query: &str, scope: &Scope) -> Result<Vec<SearchHit>> {
    let index = ensure_index(scope)?;
    let schema = index.schema();
    let f_text = field(&schema, "text")?;
    let (f_doc, f_node, f_snip) = (field(&schema, "doc")?, field(&schema, "node")?, field(&schema, "snip")?);

    let searcher = index.reader().map_err(terr)?.searcher();
    let q = QueryParser::for_index(&index, vec![f_text])
        .parse_query(query)
        .map_err(|e| Error::Invalid(format!("query: {e}")))?;
    let top = searcher
        .search(&*q, &TopDocs::with_limit(20).order_by_score())
        .map_err(terr)?;

    let mut hits = Vec::new();
    for (_score, addr) in top {
        let d: TantivyDocument = searcher.doc(addr).map_err(terr)?;
        let get = |f| d.get_first(f).and_then(|v| v.as_str()).unwrap_or("").to_string();
        hits.push(SearchHit {
            doc: Ref(get(f_doc)),
            node: Ref(get(f_node)),
            snippet: get(f_snip),
        });
    }
    Ok(hits)
}

/// Force a full rebuild of the index for `scope`; returns the node count indexed.
pub fn reindex(scope: &Scope) -> Result<usize> {
    let dir = index_dir(scope);
    let fp = fingerprint(scope)?;
    Ok(rebuild(&dir, scope, &fp)?.1)
}

// --- index lifecycle -------------------------------------------------------

fn ensure_index(scope: &Scope) -> Result<Index> {
    let dir = index_dir(scope);
    let fp = fingerprint(scope)?;
    if dir.join("meta.json").exists() && read_fingerprint(&dir).as_ref() == Some(&fp) {
        Index::open_in_dir(&dir).map_err(terr)
    } else {
        Ok(rebuild(&dir, scope, &fp)?.0)
    }
}

fn rebuild(dir: &Path, scope: &Scope, fp: &Fingerprint) -> Result<(Index, usize)> {
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).map_err(io)?;

    let index = Index::create_in_dir(dir, build_schema()).map_err(terr)?;
    let schema = index.schema();
    let f_text = field(&schema, "text")?;
    let (f_doc, f_node, f_snip) = (field(&schema, "doc")?, field(&schema, "node")?, field(&schema, "snip")?);
    let mut writer = index.writer(15_000_000).map_err(terr)?;

    let mut count = 0usize;
    for path in scope.documents()? {
        let document = match Document::open(&path) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let doc_ref = Ref::document(&document.manifest.doc_id);
        for (nid, _ty, text) in document.plaintext_nodes()? {
            let node_ref = Ref::node(&document.manifest.doc_id, &NodeId(nid));
            writer
                .add_document(doc!(
                    f_text => text.clone(),
                    f_doc => doc_ref.0.clone(),
                    f_node => node_ref.0,
                    f_snip => snippet(&text),
                ))
                .map_err(terr)?;
            count += 1;
        }
    }
    writer.commit().map_err(terr)?;
    let bytes = serde_json::to_vec(fp).map_err(|e| Error::Parse(e.to_string()))?;
    std::fs::write(dir.join("fingerprint.json"), bytes).map_err(io)?;
    Ok((index, count))
}

fn build_schema() -> Schema {
    let mut sb = Schema::builder();
    sb.add_text_field("text", TEXT);
    sb.add_text_field("doc", STORED);
    sb.add_text_field("node", STORED);
    sb.add_text_field("snip", STORED);
    sb.build()
}

fn field(schema: &Schema, name: &str) -> Result<Field> {
    schema
        .get_field(name)
        .map_err(|_| Error::Invalid(format!("index schema missing field '{name}'")))
}

// --- fingerprint + cache location ------------------------------------------

fn fingerprint(scope: &Scope) -> Result<Fingerprint> {
    let mut fp = Vec::new();
    for p in scope.documents()? {
        let md = std::fs::metadata(&p).map_err(io)?;
        let mtime = md
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        fp.push((p.to_string_lossy().to_string(), mtime, md.len()));
    }
    fp.sort();
    Ok(fp)
}

fn read_fingerprint(dir: &Path) -> Option<Fingerprint> {
    let bytes = std::fs::read(dir.join("fingerprint.json")).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn index_dir(scope: &Scope) -> PathBuf {
    let abs = std::fs::canonicalize(&scope.0).unwrap_or_else(|_| scope.0.clone());
    let hex: String = Sha256::digest(abs.to_string_lossy().as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    cache_root()
        .join("karanga")
        .join("index")
        .join(&hex[..16])
}

fn cache_root() -> PathBuf {
    if let Ok(x) = std::env::var("XDG_CACHE_HOME") {
        if !x.is_empty() {
            return PathBuf::from(x);
        }
    }
    if let Ok(h) = std::env::var("HOME") {
        if !h.is_empty() {
            return PathBuf::from(h).join(".cache");
        }
    }
    std::env::temp_dir()
}

fn snippet(text: &str) -> String {
    let t = text.trim();
    if t.chars().count() > 120 {
        format!("{}…", t.chars().take(120).collect::<String>())
    } else {
        t.to_string()
    }
}

fn terr<E: std::fmt::Display>(e: E) -> Error {
    Error::Io(format!("tantivy: {e}"))
}

fn io(e: std::io::Error) -> Error {
    Error::Io(e.to_string())
}
