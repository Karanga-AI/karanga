//! Cross-document discovery: `find` (title/description) and `search` (content).

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering};

use krg_core::query;
use krg_core::scope::Scope;
use krg_core::workspace::{Place, Workspace};

static N: AtomicU32 = AtomicU32::new(0);

fn scratch_dir(tag: &str) -> std::path::PathBuf {
    let mut d = std::env::temp_dir();
    d.push(format!(
        "krg-disc-{}-{}-{}",
        tag,
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Author a one-paragraph doc and save it as `<dir>/<name>.krg`.
fn make_doc(dir: &std::path::Path, name: &str, title: &str, body: &str) {
    let work = dir.join(format!(".{name}.work"));
    let mut ws = Workspace::create(&work, title, None).unwrap();
    ws.insert(Place::Root, "paragraph", body, BTreeMap::new()).unwrap();
    ws.save(dir.join(format!("{name}.krg"))).unwrap();
    let _ = std::fs::remove_dir_all(&work);
}

#[test]
fn find_matches_title() {
    let dir = scratch_dir("find");
    make_doc(&dir, "a", "Gateway Retry Policy", "exponential backoff");
    make_doc(&dir, "b", "Rate Limiting", "token bucket");

    let hits = query::find_documents("gateway", &Scope::new(&dir), 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].title, "Gateway Retry Policy");

    // empty query lists all
    assert_eq!(query::find_documents("", &Scope::new(&dir), 10).unwrap().len(), 2);

    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(feature = "search")]
#[test]
fn search_matches_content() {
    let dir = scratch_dir("search");
    make_doc(&dir, "a", "Gateway", "Retries use exponential backoff with jitter.");
    make_doc(&dir, "b", "Limits", "Clients are limited by a token bucket.");

    let hits = query::search("backoff", &Scope::new(&dir)).unwrap();
    assert!(!hits.is_empty(), "expected a hit for 'backoff'");
    assert!(hits.iter().any(|h| h.snippet.contains("backoff")));

    // a term in the other doc
    let bucket = query::search("bucket", &Scope::new(&dir)).unwrap();
    assert!(bucket.iter().any(|h| h.snippet.contains("token bucket")));

    let _ = std::fs::remove_dir_all(&dir);
}
