//! `krg` — Karanga command-line reader/writer (interface §7).
//!
//! Read: `outline`, `get`, `render`, `section`, `validate`.
//! Write: `new`, `insert`, `update`, `delete` (each is an ephemeral session —
//! explode the `.krg`, apply the edit, repack).

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::PathBuf;

use krg_core::document::Document;
use krg_core::model::Value;
use krg_core::workspace::{Cas, Place, Workspace};
use krg_core::{Error, Rev, Result};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let outcome = match args.first().map(String::as_str) {
        Some("outline") => cmd_outline(&args),
        Some("get") => cmd_get(&args),
        Some("render") => cmd_render(&args),
        Some("section") => cmd_section(&args),
        Some("validate") => cmd_validate(&args),
        Some("new") => cmd_new(&args),
        Some("insert") => cmd_insert(&args),
        Some("update") => cmd_update(&args),
        Some("delete") => cmd_delete(&args),
        Some("find") => cmd_find(&args),
        Some("search") => cmd_search(&args),
        Some("reindex") => cmd_reindex(&args),
        Some("nodes") => cmd_nodes(&args),
        Some("links") => cmd_links(&args),
        Some("move") => cmd_move(&args),
        Some("set-link") => cmd_set_link(&args),
        Some("remove-link") => cmd_remove_link(&args),
        Some("add-media") => cmd_add_media(&args),
        _ => {
            print!("{}", usage());
            return;
        }
    };
    if let Err(e) = outcome {
        eprintln!("krg: {e}");
        std::process::exit(1);
    }
}

// --- read ------------------------------------------------------------------

fn cmd_outline(args: &[String]) -> Result<()> {
    let path = req(args.get(1), "outline <doc>")?;
    print!("{}", Document::open(path)?.outline());
    Ok(())
}

fn cmd_get(args: &[String]) -> Result<()> {
    let path = req(args.get(1), "get <doc> <node-id>")?;
    let id = req(args.get(2), "get <doc> <node-id>")?;
    let node = Document::open(path)?.node(id)?;
    eprintln!("# {} {} (rev {})", node.ty, node.r.0, node.rev.0);
    println!("{}", node.content);
    Ok(())
}

fn cmd_render(args: &[String]) -> Result<()> {
    let path = req(args.get(1), "render <doc>")?;
    print!("{}", Document::open(path)?.render()?);
    Ok(())
}

fn cmd_section(args: &[String]) -> Result<()> {
    let path = req(args.get(1), "section <doc> <id>")?;
    let id = req(args.get(2), "section <doc> <id>")?;
    print!("{}", Document::open(path)?.section(id)?);
    Ok(())
}

fn cmd_nodes(args: &[String]) -> Result<()> {
    let (pos, kv, _) = split(&args[1..]);
    let doc = req(pos.first(), "nodes <doc> [--type <t>]")?;
    for (id, ty, label) in Document::open(doc)?.find_nodes(kv.get("--type").map(String::as_str)) {
        println!("{id}\t{ty}\t{}", label.unwrap_or_default());
    }
    Ok(())
}

fn cmd_links(args: &[String]) -> Result<()> {
    let (pos, _, flags) = split(&args[1..]);
    let doc = req(pos.first(), "links <doc> <id> [--in|--out|--both]")?;
    let id = req(pos.get(1), "links <doc> <id> [--in|--out|--both]")?;
    let dir = if flags.contains("--in") {
        krg_core::query::Direction::In
    } else if flags.contains("--both") {
        krg_core::query::Direction::Both
    } else {
        krg_core::query::Direction::Out
    };
    for l in Document::open(doc)?.get_links(id, dir)? {
        println!("{}\t{}\t{}", l.from.0, l.ty, l.to.0);
    }
    Ok(())
}

fn cmd_validate(args: &[String]) -> Result<()> {
    let path = req(args.get(1), "validate <doc>")?;
    let issues = Document::open(path)?.validate()?;
    if issues.is_empty() {
        println!("valid");
        Ok(())
    } else {
        for i in &issues {
            eprintln!("✗ {i}");
        }
        eprintln!("{} issue(s)", issues.len());
        std::process::exit(1);
    }
}

// --- write -----------------------------------------------------------------

fn cmd_new(args: &[String]) -> Result<()> {
    let (pos, kv, _) = split(&args[1..]);
    let title = req(pos.first(), "new <title> <out.krg> [--desc <text>]")?;
    let out = req(pos.get(1), "new <title> <out.krg> [--desc <text>]")?;
    let wd = work_dir();
    let _guard = WorkDir(wd.clone());
    let ws = Workspace::create(&wd, title, kv.get("--desc").map(String::as_str))?;
    ws.save(out)?;
    println!("created {} -> {}", ws.doc_ref().0, out);
    Ok(())
}

fn cmd_insert(args: &[String]) -> Result<()> {
    let usage = "insert <doc> <type> [content] [--under <id>] [--level <n>] [--lang <l>] [--ordered]";
    let (pos, kv, flags) = split(&args[1..]);
    let doc = req(pos.first(), usage)?;
    let ty = req(pos.get(1), usage)?;
    let content = match pos.get(2) {
        Some(c) => c.clone(),
        None if needs_content(ty) => read_stdin()?,
        None => String::new(),
    };
    let mut attrs: BTreeMap<String, Value> = BTreeMap::new();
    if let Some(l) = kv.get("--level") {
        attrs.insert(
            "level".into(),
            Value::Int(l.parse().map_err(|_| Error::Invalid("--level must be an integer".into()))?),
        );
    }
    if let Some(l) = kv.get("--lang") {
        attrs.insert("language".into(), Value::Str(l.clone()));
    }
    if flags.contains("--ordered") {
        attrs.insert("ordered".into(), Value::Bool(true));
    }
    let place = match kv.get("--under") {
        Some(p) => Place::Under(p.clone()),
        None => Place::Root,
    };

    let (mut ws, _guard) = open_for_edit(doc)?;
    let (id, rev) = ws.insert(place, ty, &content, attrs)?;
    ws.save(doc)?;
    println!("{id}\t{}", rev.0);
    Ok(())
}

fn cmd_update(args: &[String]) -> Result<()> {
    let usage = "update <doc> <id> <rev> [content]";
    let (pos, _, _) = split(&args[1..]);
    let doc = req(pos.first(), usage)?;
    let id = req(pos.get(1), usage)?;
    let rev = req(pos.get(2), usage)?;
    let content = match pos.get(3) {
        Some(c) => c.clone(),
        None => read_stdin()?,
    };
    let (mut ws, _guard) = open_for_edit(doc)?;
    match ws.update(id, &content, &Rev(rev.clone()))? {
        Cas::Ok(rev) => {
            ws.save(doc)?;
            println!("updated\t{}", rev.map(|r| r.0).unwrap_or_default());
            Ok(())
        }
        Cas::Stale { current_rev, current } => stale(&current_rev.0, &current),
    }
}

fn cmd_delete(args: &[String]) -> Result<()> {
    let usage = "delete <doc> <id> <rev>";
    let (pos, _, _) = split(&args[1..]);
    let doc = req(pos.first(), usage)?;
    let id = req(pos.get(1), usage)?;
    let rev = req(pos.get(2), usage)?;
    let (mut ws, _guard) = open_for_edit(doc)?;
    match ws.delete(id, &Rev(rev.clone()))? {
        Cas::Ok(_) => {
            ws.save(doc)?;
            println!("deleted {id}");
            Ok(())
        }
        Cas::Stale { current_rev, current } => stale(&current_rev.0, &current),
    }
}

fn cmd_move(args: &[String]) -> Result<()> {
    let usage = "move <doc> <id> <rev> [--under <parent>]";
    let (pos, kv, _) = split(&args[1..]);
    let doc = req(pos.first(), usage)?;
    let id = req(pos.get(1), usage)?;
    let rev = req(pos.get(2), usage)?;
    let place = match kv.get("--under") {
        Some(p) => Place::Under(p.clone()),
        None => Place::Root,
    };
    let (mut ws, _guard) = open_for_edit(doc)?;
    match ws.move_node(id, place, &Rev(rev.clone()))? {
        Cas::Ok(_) => {
            ws.save(doc)?;
            println!("moved {id}");
            Ok(())
        }
        Cas::Stale { current_rev, current } => stale(&current_rev.0, &current),
    }
}

fn cmd_set_link(args: &[String]) -> Result<()> {
    let usage = "set-link <doc> <from-id> <to-ref> <type>";
    let (pos, _, _) = split(&args[1..]);
    let doc = req(pos.first(), usage)?;
    let from = req(pos.get(1), usage)?;
    let to = req(pos.get(2), usage)?;
    let ty = req(pos.get(3), usage)?;
    let (mut ws, _guard) = open_for_edit(doc)?;
    ws.set_link(from, to, ty)?;
    ws.save(doc)?;
    println!("linked {from} -{ty}-> {to}");
    Ok(())
}

fn cmd_remove_link(args: &[String]) -> Result<()> {
    let usage = "remove-link <doc> <from-id> <to-ref> <type>";
    let (pos, _, _) = split(&args[1..]);
    let doc = req(pos.first(), usage)?;
    let from = req(pos.get(1), usage)?;
    let to = req(pos.get(2), usage)?;
    let ty = req(pos.get(3), usage)?;
    let (mut ws, _guard) = open_for_edit(doc)?;
    ws.remove_link(from, to, ty)?;
    ws.save(doc)?;
    println!("unlinked {from} -{ty}-> {to}");
    Ok(())
}

fn cmd_add_media(args: &[String]) -> Result<()> {
    let usage = "add-media <doc> <kind> <source> [--under <id>] [--alt <t>] [--caption <t>]";
    let (pos, kv, _) = split(&args[1..]);
    let doc = req(pos.first(), usage)?;
    let kind = req(pos.get(1), usage)?;
    let source = req(pos.get(2), usage)?;
    let place = match kv.get("--under") {
        Some(p) => Place::Under(p.clone()),
        None => Place::Root,
    };
    let (mut ws, _guard) = open_for_edit(doc)?;
    let (id, rev) = ws.add_media(
        place,
        kind,
        source,
        kv.get("--alt").map(String::as_str),
        kv.get("--caption").map(String::as_str),
    )?;
    ws.save(doc)?;
    println!("{id}\t{}", rev.0);
    Ok(())
}

fn stale(current_rev: &str, current: &str) -> Result<()> {
    eprintln!("✗ stale write rejected — node changed since you read it.");
    eprintln!("  current rev: {current_rev}");
    eprintln!("  current content:\n{current}");
    std::process::exit(1);
}

// --- discovery -------------------------------------------------------------

fn cmd_find(args: &[String]) -> Result<()> {
    let (pos, kv, _) = split(&args[1..]);
    let query = req(pos.first(), "find <query> [dir]")?;
    let dir = pos.get(1).map(String::as_str).unwrap_or(".");
    let limit = kv.get("--limit").and_then(|s| s.parse().ok()).unwrap_or(10);
    let hits = krg_core::query::find_documents(query, &krg_core::scope::Scope::new(dir), limit)?;
    for h in hits {
        let desc = h.description.map(|d| format!(" — {d}")).unwrap_or_default();
        println!("{}\t{}{}", h.r.0, h.title, desc);
    }
    Ok(())
}

fn cmd_search(args: &[String]) -> Result<()> {
    let (pos, _, _) = split(&args[1..]);
    let query = req(pos.first(), "search <query> [dir]")?;
    let dir = pos.get(1).map(String::as_str).unwrap_or(".");
    let hits = krg_core::query::search(query, &krg_core::scope::Scope::new(dir))?;
    for h in hits {
        println!("{}\t{}\t{}", h.node.0, h.doc.0, h.snippet);
    }
    Ok(())
}

fn cmd_reindex(args: &[String]) -> Result<()> {
    let (pos, _, _) = split(&args[1..]);
    let dir = pos.first().map(String::as_str).unwrap_or(".");
    let n = krg_core::query::reindex(&krg_core::scope::Scope::new(dir))?;
    println!("indexed {n} node(s)");
    Ok(())
}

// --- ephemeral edit session ------------------------------------------------

/// A temp working directory removed when dropped.
struct WorkDir(PathBuf);
impl Drop for WorkDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn work_dir() -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("krg-edit-{}-{}", std::process::id(), nanos))
}

fn open_for_edit(doc: &str) -> Result<(Workspace, WorkDir)> {
    let wd = work_dir();
    let ws = Workspace::open_packed(doc, &wd)?;
    Ok((ws, WorkDir(wd)))
}

// --- arg helpers -----------------------------------------------------------

fn req<'a>(v: Option<&'a String>, usage: &str) -> Result<&'a String> {
    v.ok_or_else(|| Error::Invalid(format!("usage: krg {usage}")))
}

/// Split args into positionals, `--key value` pairs, and boolean `--flags`.
fn split(args: &[String]) -> (Vec<String>, BTreeMap<String, String>, BTreeSet<String>) {
    const BOOLS: &[&str] = &["--ordered", "--in", "--out", "--both"];
    let mut pos = Vec::new();
    let mut kv = BTreeMap::new();
    let mut flags = BTreeSet::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a.starts_with("--") {
            if BOOLS.contains(&a.as_str()) {
                flags.insert(a.clone());
            } else {
                i += 1;
                if let Some(v) = args.get(i) {
                    kv.insert(a.clone(), v.clone());
                }
            }
        } else {
            pos.push(a.clone());
        }
        i += 1;
    }
    (pos, kv, flags)
}

fn needs_content(ty: &str) -> bool {
    matches!(ty, "heading" | "paragraph" | "table-cell" | "code")
}

fn read_stdin() -> Result<String> {
    let mut s = String::new();
    std::io::stdin()
        .read_to_string(&mut s)
        .map_err(|e| Error::Io(e.to_string()))?;
    Ok(s.trim_end_matches('\n').to_string())
}

fn usage() -> String {
    format!(
        "krg — Karanga document tool (format v{}, partial)\n\n{}",
        krg_core::FORMAT_VERSION,
        BODY
    )
}

const BODY: &str = "\
USAGE:
    krg <command> [args]

READ:
    outline <doc>              document outline (tier 2)
    get <doc> <id>             one rendered node (tier 3)
    render <doc>               render the whole document
    section <doc> <id>         render a section subtree
    nodes <doc> [--type <t>]   list nodes (optionally by segment type)
    links <doc> <id> [--in|--out|--both]   traverse the link graph
    validate <doc>             check hashes + structure

DISCOVER  (across a directory of .krg files):
    find <query> [dir]         match document title/description (tier 1)
    search <query> [dir]       full-text search of node content
    reindex [dir]              rebuild the persistent search index

WRITE  (operate on a .krg in place):
    new <title> <out.krg> [--desc <t>]
    insert <doc> <type> [content] [--under <id>] [--level <n>] [--lang <l>] [--ordered]
    update <doc> <id> <rev> [content]
    delete <doc> <id> <rev>
    move <doc> <id> <rev> [--under <parent>]
    set-link <doc> <from-id> <to-ref> <type>
    remove-link <doc> <from-id> <to-ref> <type>
    add-media <doc> <kind> <source> [--under <id>] [--alt <t>] [--caption <t>]

    Content may be given inline or piped on stdin. <rev> comes from `krg get`.

<doc> is a .krg file (read commands also accept an exploded directory).
";
