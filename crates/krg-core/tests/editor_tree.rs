//! The editor save contract: `Document::to_tree` ⇄ `Workspace::set_tree`,
//! id preservation, and link survival across an edit.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering};

use krg_core::document::Document;
use krg_core::query::Direction;
use krg_core::workspace::{Place, Workspace};

static N: AtomicU32 = AtomicU32::new(0);

fn scratch(tag: &str) -> std::path::PathBuf {
    let mut d = std::env::temp_dir();
    d.push(format!("krg-ed-{}-{}-{}", tag, std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn level(n: i64) -> BTreeMap<String, krg_core::model::Value> {
    let mut m = BTreeMap::new();
    m.insert("level".to_string(), krg_core::model::Value::Int(n));
    m
}

#[test]
fn edit_preserves_ids_and_links() {
    let work = scratch("work");
    let mut ws = Workspace::create(&work, "Doc", None).unwrap();
    let h = ws.insert(Place::Root, "heading", "Title", level(1)).unwrap().0;
    let (p, _) = ws.insert(Place::Under(h.clone()), "paragraph", "original", BTreeMap::new()).unwrap();
    // a link that points at the paragraph; it must survive the edit
    ws.set_link(&h, &format!("krg:///{p}"), "notes").unwrap();
    ws.save(scratch("v1").with_extension("krg")).unwrap();

    // Engine → editor tree.
    let krg1 = scratch("t1").with_extension("krg");
    ws.save(&krg1).unwrap();
    let mut tree = Document::open(&krg1).unwrap().to_tree().unwrap();
    assert_eq!(tree.len(), 1); // the heading at root
    assert_eq!(tree[0].id.as_deref(), Some(h.as_str()));
    assert_eq!(tree[0].children[0].id.as_deref(), Some(p.as_str()));

    // The "editor" edits the paragraph's text (keeping its id) and appends a
    // brand-new paragraph (no id) in the same section.
    tree[0].children[0].content = "edited text".into();
    tree[0].children.push(krg_core::tree::EditorBlock {
        id: None,
        ty: "paragraph".into(),
        content: "freshly typed".into(),
        attrs: BTreeMap::new(),
        children: vec![],
    });

    // Editor → engine.
    ws.set_tree(tree).unwrap();
    let krg2 = scratch("t2").with_extension("krg");
    ws.save(&krg2).unwrap();

    let doc = Document::open(&krg2).unwrap();
    assert!(doc.validate().unwrap().is_empty(), "{:#?}", doc.validate());

    // The paragraph kept its id and shows the new text.
    let edited = doc.node(&p).unwrap();
    assert_eq!(edited.content, "edited text");
    // The link still resolves (target id unchanged).
    let links = doc.get_links(&h, Direction::Out).unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].to.0, format!("krg:///{p}"));
    // The new paragraph exists (3 nodes total: heading + 2 paragraphs).
    assert_eq!(doc.find_nodes(Some("paragraph")).len(), 2);

    let _ = std::fs::remove_dir_all(&work);
}

#[test]
fn deleting_a_block_in_the_tree_removes_its_node() {
    let work = scratch("del-work");
    let mut ws = Workspace::create(&work, "Doc", None).unwrap();
    ws.insert(Place::Root, "paragraph", "keep", BTreeMap::new()).unwrap();
    let (drop_id, _) = ws.insert(Place::Root, "paragraph", "drop", BTreeMap::new()).unwrap();

    let krg = scratch("del").with_extension("krg");
    ws.save(&krg).unwrap();
    let mut tree = Document::open(&krg).unwrap().to_tree().unwrap();
    tree.retain(|b| b.id.as_deref() != Some(drop_id.as_str()));
    ws.set_tree(tree).unwrap();
    ws.save(&krg).unwrap();

    let doc = Document::open(&krg).unwrap();
    assert!(doc.validate().unwrap().is_empty());
    assert!(doc.node(&drop_id).is_err(), "dropped node should be gone");
    assert_eq!(doc.find_nodes(Some("paragraph")).len(), 1);

    let _ = std::fs::remove_dir_all(&work);
    let _ = std::fs::remove_file(&krg);
}
