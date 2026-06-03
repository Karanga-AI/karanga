//! move / links / add-media / find_nodes.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering};

use krg_core::document::Document;
use krg_core::query::Direction;
use krg_core::workspace::{Cas, Place, Workspace};

static N: AtomicU32 = AtomicU32::new(0);

fn scratch(tag: &str) -> std::path::PathBuf {
    let mut d = std::env::temp_dir();
    d.push(format!("krg-edit-{}-{}-{}", tag, std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn level(n: i64) -> BTreeMap<String, krg_core::model::Value> {
    let mut m = BTreeMap::new();
    m.insert("level".to_string(), krg_core::model::Value::Int(n));
    m
}

#[test]
fn move_node_relocates_subtree() {
    let work = scratch("move-w");
    let mut ws = Workspace::create(&work, "Move", None).unwrap();
    let a = ws.insert(Place::Root, "heading", "A", level(1)).unwrap().0;
    let b = ws.insert(Place::Root, "heading", "B", level(1)).unwrap().0;
    let (p, prev) = ws.insert(Place::Under(a.clone()), "paragraph", "para", BTreeMap::new()).unwrap();

    // Move paragraph from section A to section B.
    assert!(matches!(ws.move_node(&p, Place::Under(b.clone()), &prev).unwrap(), Cas::Ok(_)));

    let krg = scratch("move").with_extension("krg");
    ws.save(&krg).unwrap();
    let doc = Document::open(&krg).unwrap();
    assert!(doc.validate().unwrap().is_empty());
    // section B now contains the paragraph; section A does not.
    assert!(doc.section(&b).unwrap().contains("para"));
    assert!(!doc.section(&a).unwrap().contains("para"));

    let _ = std::fs::remove_dir_all(&work);
    let _ = std::fs::remove_file(&krg);
}

#[test]
fn links_round_trip() {
    let work = scratch("link-w");
    let mut ws = Workspace::create(&work, "Links", None).unwrap();
    let (a, _) = ws.insert(Place::Root, "paragraph", "a", BTreeMap::new()).unwrap();
    let (b, _) = ws.insert(Place::Root, "paragraph", "b", BTreeMap::new()).unwrap();
    let to_b = format!("krg:///{b}");
    ws.set_link(&a, &to_b, "relates").unwrap();
    ws.set_link(&a, &to_b, "relates").unwrap(); // idempotent

    let krg = scratch("link").with_extension("krg");
    ws.save(&krg).unwrap();
    let doc = Document::open(&krg).unwrap();

    let out = doc.get_links(&a, Direction::Out).unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].ty, "relates");
    // incoming to b (matched via short krg:/// form)
    assert_eq!(doc.get_links(&b, Direction::In).unwrap().len(), 1);

    let _ = std::fs::remove_dir_all(&work);
    let _ = std::fs::remove_file(&krg);
}

#[test]
fn add_embedded_media() {
    let work = scratch("media-w");
    // a tiny source file to embed
    let src = scratch("media-src");
    std::fs::create_dir_all(&src).unwrap();
    let img = src.join("pic.png");
    std::fs::write(&img, b"\x89PNG\r\n\x1a\n fake").unwrap();

    let mut ws = Workspace::create(&work, "Media", None).unwrap();
    let (m, _) = ws
        .add_media(Place::Root, "image", img.to_str().unwrap(), Some("alt text"), Some("a caption"))
        .unwrap();

    let krg = scratch("media").with_extension("krg");
    ws.save(&krg).unwrap();
    let doc = Document::open(&krg).unwrap();
    assert!(doc.validate().unwrap().is_empty());
    // renders as an image with caption, asset resolved to media/<id>.png
    let rendered = doc.node(&m).unwrap().content;
    assert!(rendered.starts_with("![alt text](media/"), "got: {rendered}");
    assert!(rendered.ends_with(".png)\n\n*a caption*"), "got: {rendered}");

    let _ = std::fs::remove_dir_all(&work);
    let _ = std::fs::remove_dir_all(&src);
    let _ = std::fs::remove_file(&krg);
}

#[test]
fn find_nodes_by_type() {
    let work = scratch("nodes-w");
    let mut ws = Workspace::create(&work, "Nodes", None).unwrap();
    ws.insert(Place::Root, "heading", "H", level(1)).unwrap();
    ws.insert(Place::Root, "paragraph", "p1", BTreeMap::new()).unwrap();
    ws.insert(Place::Root, "paragraph", "p2", BTreeMap::new()).unwrap();
    let krg = scratch("nodes").with_extension("krg");
    ws.save(&krg).unwrap();

    let doc = Document::open(&krg).unwrap();
    assert_eq!(doc.find_nodes(Some("paragraph")).len(), 2);
    assert_eq!(doc.find_nodes(Some("heading")).len(), 1);
    assert_eq!(doc.find_nodes(None).len(), 3);

    let _ = std::fs::remove_dir_all(&work);
    let _ = std::fs::remove_file(&krg);
}
