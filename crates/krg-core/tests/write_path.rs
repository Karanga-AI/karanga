//! Write-path integration tests: author a document, round-trip through a packed
//! `.krg`, and exercise optimistic CAS.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering};

use krg_core::document::Document;
use krg_core::model::Value;
use krg_core::workspace::{Cas, Place, Workspace};

static N: AtomicU32 = AtomicU32::new(0);

/// Unique scratch directory per test invocation.
fn scratch(tag: &str) -> std::path::PathBuf {
    let mut d = std::env::temp_dir();
    d.push(format!(
        "krg-wtest-{}-{}-{}",
        tag,
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn level(n: i64) -> BTreeMap<String, Value> {
    let mut m = BTreeMap::new();
    m.insert("level".to_string(), Value::Int(n));
    m
}

#[test]
fn author_round_trip_through_krg() {
    let work = scratch("rt-work");
    let mut ws = Workspace::create(&work, "My Doc", Some("a test")).unwrap();
    let h = ws.insert(Place::Root, "heading", "Intro", level(1)).unwrap().0;
    ws.insert(
        Place::Under(h.clone()),
        "paragraph",
        "Hello **world** and a [link](https://example.com).",
        BTreeMap::new(),
    )
    .unwrap();

    let krg = scratch("rt").with_extension("krg");
    ws.save(&krg).unwrap();

    // Reopen the packed file with the read path.
    let doc = Document::open(&krg).unwrap();
    assert_eq!(doc.manifest.title, "My Doc");

    // Hashes written during authoring must verify on read.
    let issues = doc.validate().unwrap();
    assert!(issues.is_empty(), "validation issues: {issues:#?}");

    // The inline marks round-trip through render.
    let md = doc.render().unwrap();
    assert!(md.contains("# Intro"), "render:\n{md}");
    assert!(
        md.contains("Hello **world** and a [link](https://example.com)."),
        "render:\n{md}"
    );

    let _ = std::fs::remove_dir_all(&work);
    let _ = std::fs::remove_file(&krg);
}

#[test]
fn optimistic_cas() {
    let work = scratch("cas-work");
    let mut ws = Workspace::create(&work, "CAS", None).unwrap();
    let (p, rev0) = ws.insert(Place::Root, "paragraph", "first", BTreeMap::new()).unwrap();

    // Correct rev → applies, returns a new rev.
    let rev1 = match ws.update(&p, "second", &rev0).unwrap() {
        Cas::Ok(Some(r)) => r,
        other => panic!("expected Ok, got stale? {}", matches!(other, Cas::Stale { .. })),
    };
    assert_ne!(rev0, rev1);

    // Stale rev (the original) → rejected, surfaces current content.
    match ws.update(&p, "third", &rev0).unwrap() {
        Cas::Stale { current_rev, current } => {
            assert_eq!(current_rev, rev1);
            assert_eq!(current, "second");
        }
        Cas::Ok(_) => panic!("stale write should not apply"),
    }

    // Fresh rev → applies again.
    assert!(matches!(ws.update(&p, "third", &rev1).unwrap(), Cas::Ok(Some(_))));

    let _ = std::fs::remove_dir_all(&work);
}

#[test]
fn delete_removes_node() {
    let work = scratch("del-work");
    let mut ws = Workspace::create(&work, "Del", None).unwrap();
    let (a, _) = ws.insert(Place::Root, "paragraph", "a", BTreeMap::new()).unwrap();
    let (b, rev_b) = ws.insert(Place::Root, "paragraph", "b", BTreeMap::new()).unwrap();

    assert!(matches!(ws.delete(&b, &rev_b).unwrap(), Cas::Ok(None)));

    let krg = scratch("del").with_extension("krg");
    ws.save(&krg).unwrap();
    let doc = Document::open(&krg).unwrap();
    assert!(doc.validate().unwrap().is_empty());
    assert!(doc.node(&a).is_ok());
    assert!(doc.node(&b).is_err(), "deleted node should be gone");

    let _ = std::fs::remove_dir_all(&work);
    let _ = std::fs::remove_file(&krg);
}
