//! `krg-convert` — conversion between `.krg` and other formats (FR-30).
//!
//! Provides interop (Markdown first; `.docx` and others as goals) so the
//! format avoids lock-in. Scaffold: not implemented yet.
#![allow(dead_code, unused_variables)]

use krg_core::Result;

/// Import a Markdown document into a new `.krg`.
pub fn import_markdown(md: &str) -> Result<()> {
    unimplemented!("markdown import")
}

/// Export a `.krg` document to Markdown.
pub fn export_markdown() -> Result<String> {
    unimplemented!("markdown export")
}
