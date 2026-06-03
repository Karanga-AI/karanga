//! `krg` — Karanga command-line reader/writer (interface §7).
//!
//! Implemented so far: `outline`, `get` (read path). Other subcommands are
//! scaffolded.

use krg_core::document::Document;
use krg_core::{Error, Result};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let outcome = match args.first().map(String::as_str) {
        Some("outline") => cmd_outline(&args),
        Some("get") => cmd_get(&args),
        Some("render") => cmd_render(&args),
        Some("section") => cmd_section(&args),
        Some(
            cmd @ ("find" | "nodes" | "search" | "links" | "new" | "insert"
            | "update" | "move" | "delete" | "set-link" | "add-media"),
        ) => {
            eprintln!("krg: '{cmd}' is not implemented yet (scaffold).");
            std::process::exit(2);
        }
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

fn cmd_outline(args: &[String]) -> Result<()> {
    let path = args
        .get(1)
        .ok_or_else(|| Error::Invalid("usage: krg outline <doc>".into()))?;
    let doc = Document::open(path)?;
    print!("{}", doc.outline());
    Ok(())
}

fn cmd_get(args: &[String]) -> Result<()> {
    let path = args
        .get(1)
        .ok_or_else(|| Error::Invalid("usage: krg get <doc> <node-id>".into()))?;
    let id = args
        .get(2)
        .ok_or_else(|| Error::Invalid("usage: krg get <doc> <node-id>".into()))?;
    let doc = Document::open(path)?;
    let node = doc.node(id)?;
    eprintln!("# {} {} (rev {})", node.ty, node.r.0, node.rev.0);
    println!("{}", node.content);
    Ok(())
}

fn cmd_render(args: &[String]) -> Result<()> {
    let path = args
        .get(1)
        .ok_or_else(|| Error::Invalid("usage: krg render <doc>".into()))?;
    print!("{}", Document::open(path)?.render()?);
    Ok(())
}

fn cmd_section(args: &[String]) -> Result<()> {
    let path = args
        .get(1)
        .ok_or_else(|| Error::Invalid("usage: krg section <doc> <id>".into()))?;
    let id = args
        .get(2)
        .ok_or_else(|| Error::Invalid("usage: krg section <doc> <id>".into()))?;
    print!("{}", Document::open(path)?.section(id)?);
    Ok(())
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
    outline <doc>              document outline (tier 2)        [implemented]
    get <doc> <id>             one rendered node (tier 3)       [implemented]
    render <doc>               render the whole document        [implemented]
    section <doc> <id>         render a section subtree         [implemented]
    find <query> [dir]         discover documents (tier 1)
    nodes <doc> --type <t>     filter nodes by segment type
    search <query> [dir]       full-text / fuzzy search
    links <doc> <id>           traverse links

WRITE:
    new <title>                create a document
    insert | update | move | delete | set-link | add-media

<doc> may be a .krg file or an exploded document directory.
";
