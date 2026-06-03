//! Block-level Markdown authoring (`insert_markdown`) + cross-document backlinks.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering};

use krg_core::document::Document;
use krg_core::query;
use krg_core::scope::Scope;
use krg_core::workspace::{Place, Workspace};

static N: AtomicU32 = AtomicU32::new(0);

fn scratch(tag: &str) -> std::path::PathBuf {
    let mut d = std::env::temp_dir();
    d.push(format!("krg-md-{}-{}-{}", tag, std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&d);
    d
}

const MD: &str = "\
# Overview

A paragraph with **bold** and a [link](https://example.com).

- first
- second
  - nested

```go
fn main() {}
```

## Details

> a quote

---
";

#[test]
fn insert_markdown_builds_structure() {
    let work = scratch("md-work");
    let mut ws = Workspace::create(&work, "MD", None).unwrap();
    ws.insert_markdown(Place::Root, MD).unwrap();

    let krg = scratch("md").with_extension("krg");
    ws.save(&krg).unwrap();
    let doc = Document::open(&krg).unwrap();
    assert!(doc.validate().unwrap().is_empty(), "{:#?}", doc.validate());

    // Section nesting: Overview (h1) and its nested Details (h2).
    let outline = doc.outline();
    assert!(outline.contains("- Overview"), "{outline}");
    assert!(outline.contains("  - Details"), "{outline}");

    // Block types all present.
    assert_eq!(doc.find_nodes(Some("heading")).len(), 2);
    assert_eq!(doc.find_nodes(Some("list")).len(), 2); // outer + nested
    assert_eq!(doc.find_nodes(Some("code")).len(), 1);
    assert_eq!(doc.find_nodes(Some("blockquote")).len(), 1);
    assert_eq!(doc.find_nodes(Some("divider")).len(), 1);

    // Inline marks survive in the rendered output.
    let rendered = doc.render().unwrap();
    assert!(rendered.contains("A paragraph with **bold** and a [link](https://example.com)."));
    assert!(rendered.contains("```go\nfn main() {}\n```"));
    assert!(rendered.contains("> a quote"));

    let _ = std::fs::remove_dir_all(&work);
    let _ = std::fs::remove_file(&krg);
}

#[test]
fn cross_document_backlinks() {
    let dir = scratch("xdoc");
    std::fs::create_dir_all(&dir).unwrap();

    // Target doc B with a node.
    let bwork = dir.join(".b.work");
    let mut b = Workspace::create(&bwork, "Target", None).unwrap();
    let (bn, _) = b.insert(Place::Root, "paragraph", "target node", BTreeMap::new()).unwrap();
    let bdoc_ref = b.doc_ref().0; // krg://<docB>/
    b.save(dir.join("b.krg")).unwrap();
    let _ = std::fs::remove_dir_all(&bwork);

    let target_ref = format!("{bdoc_ref}{bn}"); // krg://<docB>/<bn>

    // Source doc A links to B's node.
    let awork = dir.join(".a.work");
    let mut a = Workspace::create(&awork, "Source", None).unwrap();
    let (an, _) = a.insert(Place::Root, "paragraph", "source", BTreeMap::new()).unwrap();
    a.set_link(&an, &target_ref, "cites").unwrap();
    a.save(dir.join("a.krg")).unwrap();
    let _ = std::fs::remove_dir_all(&awork);

    // Backlinks to B's node, scanning the directory, find the link from A.
    let back = query::backlinks(&target_ref, &Scope::new(&dir)).unwrap();
    assert_eq!(back.len(), 1, "{back:#?}");
    assert_eq!(back[0].from.0, an);
    assert_eq!(back[0].ty, "cites");

    let _ = std::fs::remove_dir_all(&dir);
}
