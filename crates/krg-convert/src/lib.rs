//! `krg-convert` — conversion between `.krg` and other formats (FR-30).
//!
//! v0.1: Markdown in/out. `export_markdown` renders a `.krg` to Karanga
//! Markdown; `import_markdown` authors a new `.krg` from a Markdown document.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use krg_core::document::Document;
use krg_core::workspace::{Place, Workspace};
use krg_core::{Error, Result};

/// Render a `.krg` document to Karanga Markdown.
pub fn export_markdown(krg: &Path) -> Result<String> {
    Document::open(krg)?.render()
}

/// Author a new `.krg` at `out` from a Markdown document.
pub fn import_markdown(md: &str, title: &str, out: &Path) -> Result<()> {
    let work = std::env::temp_dir().join(format!(
        "krg-import-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let result = (|| {
        let mut ws = Workspace::create(&work, title, None)?;
        ws.insert_markdown(Place::Root, md)?;
        ws.save(out)
    })();
    let _ = std::fs::remove_dir_all(&work);
    result.map_err(|e: Error| e)
}
