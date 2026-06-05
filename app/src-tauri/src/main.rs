//! Karanga desktop editor — Tauri backend.
//!
//! Commands bridge the webview editor to `krg-core` in-process. v0.1 uses a
//! whole-document Markdown round-trip: `open_document` renders a `.krg` to
//! Karanga Markdown for the editor, and `save_document` re-authors the `.krg`
//! from the edited Markdown (preserving the doc's `doc_id`/title; node ids are
//! regenerated — id-stable editing via `Workspace::set_tree` is the next step).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use krg_core::document::Document;
use krg_core::workspace::Workspace;
use serde::Serialize;

#[derive(Serialize)]
struct DocPayload {
    markdown: String,
    title: String,
}

/// Open a `.krg` and return it as Karanga Markdown + its title.
#[tauri::command]
fn open_document(path: String) -> Result<DocPayload, String> {
    let doc = Document::open(&path).map_err(|e| e.to_string())?;
    let markdown = doc.render().map_err(|e| e.to_string())?;
    Ok(DocPayload {
        markdown,
        title: doc.manifest.title.clone(),
    })
}

/// Write a `.krg` at `path` from the editor's Markdown. Reuses the existing
/// document's `doc_id` when the file already exists; otherwise creates one.
#[tauri::command]
fn save_document(path: String, title: String, markdown: String) -> Result<(), String> {
    let work = std::env::temp_dir().join(format!(
        "krg-app-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let result = (|| -> krg_core::Result<()> {
        let mut ws = if Path::new(&path).exists() {
            Workspace::open_packed(&path, &work)?
        } else {
            Workspace::create(&work, &title, None)?
        };
        ws.set_title(&title)?;
        ws.replace_with_markdown(&markdown)?;
        ws.save(&path)
    })();
    let _ = std::fs::remove_dir_all(&work);
    result.map_err(|e| e.to_string())
}

/// Report the node configuration the current editor Markdown would produce
/// (the debug inspector). No file is written.
#[tauri::command]
fn inspect_document(markdown: String) -> Vec<krg_core::tree::InspectNode> {
    krg_core::tree::inspect_markdown(&markdown)
}

/// Disable macOS "smart" dash/quote substitution inside the WKWebView.
/// WebKit's text checker reads these per-app `NSUserDefaults` keys (the same
/// ones Safari's Edit → Substitutions menu persists); they must be set before
/// the webview is created. System-level substitution would otherwise rewrite
/// the markdown the user types (`---` → em-dash, straight → curly quotes).
#[cfg(target_os = "macos")]
fn disable_smart_substitutions() {
    use objc2_foundation::{ns_string, NSUserDefaults};
    let defaults = NSUserDefaults::standardUserDefaults();
    defaults.setBool_forKey(false, ns_string!("WebAutomaticDashSubstitutionEnabled"));
    defaults.setBool_forKey(false, ns_string!("WebAutomaticQuoteSubstitutionEnabled"));
}

fn main() {
    #[cfg(target_os = "macos")]
    disable_smart_substitutions();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            open_document,
            save_document,
            inspect_document
        ])
        .run(tauri::generate_context!())
        .expect("error while running Karanga");
}
