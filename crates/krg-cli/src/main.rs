//! `krg` — Karanga command-line reader/writer (interface §7).
//!
//! Scaffold: prints usage; subcommands are not implemented yet.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("find") | Some("outline") | Some("get") | Some("section") | Some("nodes")
        | Some("search") | Some("links") | Some("new") | Some("insert") | Some("update")
        | Some("move") | Some("delete") | Some("set-link") | Some("add-media") => {
            eprintln!("krg: '{}' is not implemented yet (scaffold).", args[0]);
            std::process::exit(2);
        }
        _ => print!("{}", usage()),
    }
}

fn usage() -> String {
    format!(
        "krg — Karanga document tool (format v{}, scaffold)\n\n{}",
        krg_core::FORMAT_VERSION,
        BODY
    )
}

const BODY: &str = "\
USAGE:
    krg <command> [args]

READ:
    find <query> [dir]         discover documents (tier 1)
    outline <file>             document outline (tier 2)
    get <file> <id>            one node (tier 3)
    section <file> <id>        a section subtree
    nodes <file> --type <t>    filter nodes by segment type
    search <query> [dir]       full-text / fuzzy search
    links <file> <id>          traverse links

WRITE:
    new <title>                create a document
    insert | update | move | delete | set-link | add-media
";
