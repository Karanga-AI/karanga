//! Full-text search over a directory of `.krg` files, backed by an **in-RAM
//! Tantivy index built per query** (interface §6).
//!
//! The index is a rebuildable, path-scoped accelerator — never authoritative,
//! never a format artifact. Persisting it across runs is later work; at local
//! corpus sizes a per-query rebuild is fine and avoids any staleness.

use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{Schema, Value, STORED, TEXT};
use tantivy::{doc, Index, TantivyDocument};

use crate::document::Document;
use crate::error::Error;
use crate::id::{NodeId, Ref};
use crate::query::SearchHit;
use crate::scope::Scope;
use crate::Result;

pub fn search(query: &str, scope: &Scope) -> Result<Vec<SearchHit>> {
    let mut sb = Schema::builder();
    let f_text = sb.add_text_field("text", TEXT);
    let f_doc = sb.add_text_field("doc", STORED);
    let f_node = sb.add_text_field("node", STORED);
    let f_snip = sb.add_text_field("snip", STORED);
    let schema = sb.build();

    let index = Index::create_in_ram(schema);
    let mut writer = index.writer(15_000_000).map_err(terr)?;

    for path in scope.documents()? {
        let document = match Document::open(&path) {
            Ok(d) => d,
            Err(_) => continue, // skip unreadable docs
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
        }
    }
    writer.commit().map_err(terr)?;

    let reader = index.reader().map_err(terr)?;
    let searcher = reader.searcher();
    let parser = QueryParser::for_index(&index, vec![f_text]);
    let q = parser
        .parse_query(query)
        .map_err(|e| Error::Invalid(format!("query: {e}")))?;
    let top = searcher
        .search(&*q, &TopDocs::with_limit(20).order_by_score())
        .map_err(terr)?;

    let mut hits = Vec::new();
    for (_score, addr) in top {
        let d: TantivyDocument = searcher.doc(addr).map_err(terr)?;
        let get = |f| {
            d.get_first(f)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        };
        hits.push(SearchHit {
            doc: Ref(get(f_doc)),
            node: Ref(get(f_node)),
            snippet: get(f_snip),
        });
    }
    Ok(hits)
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
