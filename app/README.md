# Karanga desktop editor

A cross-platform WYSIWYG editor for `.krg` documents, built with **Tauri 2**
(Rust backend linking `krg-core` in-process) + **TipTap** (ProseMirror) in the
webview. You type Markdown and it renders live as you type — Obsidian-style.

## Prerequisites

- **Rust** (stable) — already needed for the engine.
- **Node ≥ 18** and **pnpm** (`npm i -g pnpm`).
- **Tauri 2 system prerequisites** for your OS — see
  <https://v2.tauri.app/start/prerequisites/>. On macOS that's just the Xcode
  Command Line Tools (`xcode-select --install`).

## Run the dev server

From `app/`:

```sh
pnpm install          # first time: fetch frontend deps
pnpm tauri dev        # launches Vite + the native window with live reload
```

`pnpm tauri dev` runs `vite` (the webview at http://localhost:1420) and builds
the Rust backend (which compiles `krg-core`). The app window opens automatically.

> First `cargo`/Tauri build pulls a large GUI dependency tree and can take a few
> minutes. Subsequent runs are fast.

## What you can do

| Action | How |
|---|---|
| **New** document | `⌘N` / `Ctrl+N`, or the *New* button |
| **Open** a `.krg` | `⌘O` / `Ctrl+O`, or *Open* (file picker) |
| **Save** | `⌘S` / `Ctrl+S` (prompts for a path the first time) |
| **Save As** | `⌘⇧S` / `Ctrl+Shift+S` (always prompts) |
| Edit | Just type. `# ` → heading, `**bold**`, `*italic*`, `` `code` ``, `- ` list, `> ` quote, ```` ``` ```` code block, `---` divider — all render live. |

The title field (top-left) sets the document title stored in the `.krg`.

## How it works

- `open_document(path)` — the backend `Document::open(path).render()`s the `.krg`
  to Karanga Markdown; the editor loads it.
- `save_document(path, title, markdown)` — the backend re-authors the `.krg` from
  the edited Markdown via `Workspace::replace_with_markdown` + `save` (a ZIP repack).

The editor is **TipTap StarterKit** (live markdown input rules — type and it
renders) + `marked` (markdown → editor on open) and `turndown` (editor → markdown
on save). If anything fails at runtime, a red banner appears at the top of the
window with the error — copy that text when reporting a problem.

## Troubleshooting

- After pulling dependency changes, re-run `pnpm install` before `pnpm tauri dev`.
- A red error banner at the top means the webview hit a runtime error; its text
  is the actual cause.

## v0.1 limitations (known, by design)

- **Whole-document Markdown round-trip.** Saving regenerates the node ids inside
  the document. The `doc_id` and title are preserved, but per-node links *within*
  a document and cross-document links *to* its nodes will not survive a save yet.
  The engine already has the id-preserving path (`Document::to_tree` ⇄
  `Workspace::set_tree`); wiring the TipTap blocks to carry their `krg` node ids
  is the planned next step and removes this limitation.
- **Markdown coverage** matches the import dialect: headings, paragraphs, lists
  (nested), code, blockquotes, dividers, and inline bold/italic/code/strike/links.
  Tables are rendered but not yet round-tripped through the editor.
- This scaffold was written without a GUI to test against; if a TipTap or Tauri
  API has shifted, expect a small fixup on first run.
