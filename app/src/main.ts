import { Editor, Extension } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import Link from "@tiptap/extension-link";
import Table from "@tiptap/extension-table";
import TableRow from "@tiptap/extension-table-row";
import TableHeader from "@tiptap/extension-table-header";
import TableCell from "@tiptap/extension-table-cell";
import { marked } from "marked";
import TurndownService from "turndown";
import { tables as gfmTables } from "turndown-plugin-gfm";
import { invoke } from "@tauri-apps/api/core";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";

// --- visible error surface (so failures aren't silent) ---------------------
const errorEl = document.querySelector<HTMLDivElement>("#error")!;
function showError(msg: string): void {
  errorEl.textContent = msg;
  errorEl.style.display = "block";
}
window.addEventListener("error", (e) => showError("Error: " + (e.error?.stack ?? e.message)));
window.addEventListener("unhandledrejection", (e) =>
  showError("Error: " + ((e.reason && (e.reason.stack ?? e.reason.message)) ?? String(e.reason))),
);

// --- markdown <-> editor ----------------------------------------------------
const turndown = new TurndownService({
  headingStyle: "atx",
  codeBlockStyle: "fenced",
  bulletListMarker: "-",
});
// GFM pipe tables on save. TipTap wraps cell content in <p>, which would put
// newlines inside pipe rows — flatten paragraphs inside th/td first.
turndown.use(gfmTables);
turndown.addRule("cellParagraph", {
  filter: (node) =>
    node.nodeName === "P" &&
    (node.parentNode?.nodeName === "TD" || node.parentNode?.nodeName === "TH"),
  replacement: (content, node) => ((node as HTMLElement).previousSibling ? " " + content : content),
});

// Typing a GFM table by hand: a paragraph of `| a | b |` followed by a
// paragraph of `| --- | --- |` becomes a real table when Enter is pressed on
// the separator row (header from the first line, plus one empty body row).
const PIPE_ROW = /\|/;
const SEPARATOR_ROW = /^\|?\s*:?-+:?\s*(\|\s*:?-+:?\s*)+\|?$/;

function splitRow(line: string): string[] {
  return line
    .trim()
    .replace(/^\|/, "")
    .replace(/\|$/, "")
    .split("|")
    .map((c) => c.trim());
}

// NOTE: macOS smart dash/quote substitution is disabled at the source — the
// Tauri backend sets WebKit's `WebAutomaticDashSubstitutionEnabled` /
// `WebAutomaticQuoteSubstitutionEnabled` NSUserDefaults before the webview
// exists (src-tauri/main.rs). Intercepting the `insertReplacementText`
// beforeinput here instead proved unreliable: WebKit mutates the DOM
// regardless and ProseMirror's reconciliation mangles the cursor.

const MarkdownTableTyping = Extension.create({
  name: "markdownTableTyping",
  addKeyboardShortcuts() {
    return {
      Enter: () => {
        const { state } = this.editor;
        const { $from, empty } = state.selection;
        if (!empty || $from.parent.type.name !== "paragraph" || $from.depth !== 1) return false;
        const sep = $from.parent.textContent;
        if (!SEPARATOR_ROW.test(sep.trim())) return false;
        const before = state.doc.childBefore($from.before(1));
        if (!before.node || before.node.type.name !== "paragraph") return false;
        const headText = before.node.textContent;
        if (!PIPE_ROW.test(headText)) return false;
        const headers = splitRow(headText);
        if (headers.length < 2 || headers.length !== splitRow(sep).length) return false;

        const { schema } = state;
        const para = (text: string) =>
          schema.nodes.paragraph.create(null, text ? schema.text(text) : null);
        const headerRow = schema.nodes.tableRow.create(
          null,
          headers.map((h) => schema.nodes.tableHeader.create(null, para(h))),
        );
        const bodyRow = schema.nodes.tableRow.create(
          null,
          headers.map(() => schema.nodes.tableCell.create(null, para(""))),
        );
        const table = schema.nodes.table.create(null, [headerRow, bodyRow]);

        const from = before.offset; // start of the header-text paragraph
        const to = $from.after(1); // end of the separator paragraph
        return this.editor
          .chain()
          .command(({ tr }) => {
            tr.replaceRangeWith(from, to, table);
            return true;
          })
          // into the first body-row cell
          .setTextSelection(from + headerRow.nodeSize + 4)
          .run();
      },
    };
  },
});

const KRG = { name: "Karanga document", extensions: ["krg"] };
let currentPath: string | null = null;
let dirty = false;

const titleEl = document.querySelector<HTMLInputElement>("#title")!;

let editor: Editor;
try {
  editor = new Editor({
    element: document.querySelector<HTMLElement>("#editor")!,
    extensions: [
      // StarterKit alone gives the Obsidian feel: typing `# `, `**bold**`,
      // `- `, `> `, ``` ``` ```, `1. `, `---` transforms live as you type.
      StarterKit,
      Link.configure({ openOnClick: false }),
      Table.configure({ resizable: false }),
      TableRow,
      TableHeader,
      TableCell,
      MarkdownTableTyping,
    ],
    content: "<p></p>",
    autofocus: true,
    onUpdate: () => {
      dirty = true;
      updateWindowTitle();
      scheduleInspect();
    },
  });
} catch (err) {
  showError("Failed to start the editor: " + String(err));
  throw err;
}

// --- contextual table controls ----------------------------------------------
// Hover affordances (Notion-style): a slim "+" strip under the table appends
// a row; one along the right edge appends a column. Destructive operations
// live in a right-click menu inside the table. (Tab in the last cell also
// appends a row — the Table extension's built-in shortcut.)
const editorEl = document.querySelector<HTMLElement>("#editor")!;
const addRowBtn = document.querySelector<HTMLButtonElement>("#add-row")!;
const addColBtn = document.querySelector<HTMLButtonElement>("#add-col")!;
const HOVER_PX = 22;

/// A text position inside the table's bottom-right cell — addRowAfter from
/// here appends a final row; addColumnAfter appends a rightmost column.
function lastCellPos(tableEl: HTMLTableElement): number | null {
  const lastRow = tableEl.rows[tableEl.rows.length - 1];
  const cell = lastRow?.cells[lastRow.cells.length - 1];
  if (!cell) return null;
  try {
    return editor.view.posAtDOM(cell, 0) + 1;
  } catch {
    return null;
  }
}

let rowTable: HTMLTableElement | null = null;
let colTable: HTMLTableElement | null = null;

function hideHoverButtons(): void {
  addRowBtn.hidden = true;
  addColBtn.hidden = true;
  rowTable = null;
  colTable = null;
}

window.addEventListener("mousemove", (e) => {
  rowTable = null;
  colTable = null;
  for (const t of editorEl.querySelectorAll("table")) {
    const r = t.getBoundingClientRect();
    const nearX = e.clientX >= r.left - HOVER_PX && e.clientX <= r.right + HOVER_PX;
    const nearY = e.clientY >= r.top - HOVER_PX && e.clientY <= r.bottom + HOVER_PX;
    if (nearX && Math.abs(e.clientY - r.bottom) <= HOVER_PX) {
      rowTable = t;
      addRowBtn.style.left = `${r.left}px`;
      addRowBtn.style.top = `${r.bottom + 3}px`;
      addRowBtn.style.width = `${r.width}px`;
      addRowBtn.style.height = "14px";
    }
    if (nearY && Math.abs(e.clientX - r.right) <= HOVER_PX) {
      colTable = t;
      addColBtn.style.left = `${r.right + 3}px`;
      addColBtn.style.top = `${r.top}px`;
      addColBtn.style.width = "14px";
      addColBtn.style.height = `${r.height}px`;
    }
  }
  addRowBtn.hidden = !rowTable;
  addColBtn.hidden = !colTable;
});
window.addEventListener("scroll", hideHoverButtons, true);

for (const [btn, getTable, cmd] of [
  [addRowBtn, () => rowTable, "addRowAfter"],
  [addColBtn, () => colTable, "addColumnAfter"],
] as const) {
  btn.addEventListener("mousedown", (e) => e.preventDefault()); // keep editor focus
  btn.addEventListener("click", () => {
    const t = getTable();
    const pos = t && lastCellPos(t);
    if (pos != null) {
      editor.chain().focus().setTextSelection(pos)[cmd]().run();
    }
    hideHoverButtons();
  });
}

// Right-click inside a table: row/column/table operations.
const tableMenu = document.querySelector<HTMLDivElement>("#table-menu")!;
const menuItems: Array<[string, () => boolean]> = [
  ["Insert row below", () => editor.chain().focus().addRowAfter().run()],
  ["Insert column right", () => editor.chain().focus().addColumnAfter().run()],
  ["Delete row", () => editor.chain().focus().deleteRow().run()],
  ["Delete column", () => editor.chain().focus().deleteColumn().run()],
  ["Toggle header row", () => editor.chain().focus().toggleHeaderRow().run()],
  ["Delete table", () => editor.chain().focus().deleteTable().run()],
];
for (const [label, run] of menuItems) {
  const b = document.createElement("button");
  b.textContent = label;
  b.addEventListener("mousedown", (e) => e.preventDefault());
  b.addEventListener("click", () => {
    tableMenu.hidden = true;
    run();
  });
  tableMenu.appendChild(b);
}
editorEl.addEventListener("contextmenu", (e) => {
  if (!(e.target as HTMLElement).closest("table")) return;
  const coords = editor.view.posAtCoords({ left: e.clientX, top: e.clientY });
  if (!coords) return;
  e.preventDefault();
  // act on the cell that was clicked, not wherever the caret happened to be
  editor.chain().setTextSelection(coords.pos).run();
  tableMenu.style.left = `${e.clientX}px`;
  tableMenu.style.top = `${e.clientY}px`;
  tableMenu.hidden = false;
});
window.addEventListener("mousedown", (e) => {
  if (!tableMenu.contains(e.target as Node)) tableMenu.hidden = true;
});
window.addEventListener("keydown", (e) => {
  if (e.key === "Escape") tableMenu.hidden = true;
});

function title(): string {
  return titleEl.value.trim() || "Untitled";
}

function getMarkdown(): string {
  // TipTap emits a <colgroup> before <tbody>; turndown-plugin-gfm's
  // heading-row detection requires tbody to be the first element child, so a
  // colgroup makes it keep the whole table as raw HTML (which the engine then
  // skips on import — the table would silently vanish from the file).
  const dom = new DOMParser().parseFromString(editor.getHTML(), "text/html");
  for (const cg of dom.querySelectorAll("colgroup")) cg.remove();
  const md = turndown.turndown(dom.body.innerHTML);
  // Loud guard against any future silent table loss in the conversion.
  if (md.includes("<table") || md.includes("</table>")) {
    showError("Internal error: a table failed to convert to markdown — NOT saving it would lose data. Please report this.");
  }
  return md;
}

function setMarkdown(md: string): void {
  const html = marked.parse(md, { async: false }) as string;
  editor.commands.setContent(html);
}

function updateWindowTitle(): void {
  const base = currentPath ? currentPath.split(/[\\/]/).pop() : title();
  document.title = `${dirty ? "• " : ""}${base} — Karanga`;
}

async function openFile(): Promise<void> {
  const path = await openDialog({ multiple: false, directory: false, filters: [KRG] });
  if (typeof path !== "string") return;
  const doc = await invoke<{ markdown: string; title: string }>("open_document", { path });
  setMarkdown(doc.markdown);
  currentPath = path;
  titleEl.value = doc.title;
  dirty = false;
  updateWindowTitle();
}

async function save(): Promise<void> {
  if (!currentPath) return saveAs();
  await invoke("save_document", { path: currentPath, title: title(), markdown: getMarkdown() });
  dirty = false;
  updateWindowTitle();
}

async function saveAs(): Promise<void> {
  const chosen = await saveDialog({ filters: [KRG], defaultPath: currentPath ?? `${title()}.krg` });
  if (typeof chosen !== "string") return;
  currentPath = chosen.endsWith(".krg") ? chosen : `${chosen}.krg`;
  await save();
}

function newFile(): void {
  editor.commands.clearContent();
  currentPath = null;
  titleEl.value = "";
  dirty = false;
  editor.commands.focus();
  updateWindowTitle();
}

document.querySelector("#new")!.addEventListener("click", () => void newFile());
document.querySelector("#open")!.addEventListener("click", () => void openFile());
document.querySelector("#save")!.addEventListener("click", () => void save());
titleEl.addEventListener("input", () => {
  dirty = true;
  updateWindowTitle();
});

// ⌘/Ctrl+S save · ⌘/Ctrl+Shift+S save as · ⌘/Ctrl+O open · ⌘/Ctrl+N new
window.addEventListener("keydown", (e) => {
  const mod = e.metaKey || e.ctrlKey;
  if (!mod) return;
  const k = e.key.toLowerCase();
  if (k === "s") {
    e.preventDefault();
    void (e.shiftKey ? saveAs() : save());
  } else if (k === "o" && !e.shiftKey) {
    e.preventDefault();
    void openFile();
  } else if (k === "n" && !e.shiftKey) {
    e.preventDefault();
    newFile();
  }
});

// --- debug node inspector ---------------------------------------------------
interface InspectNode {
  path: string;
  type: string;
  hash: string;
  node: unknown;
  children: InspectNode[];
}

const debugEl = document.querySelector<HTMLElement>("#debug")!;
let debugOn = false;
let inspectTimer: ReturnType<typeof setTimeout> | undefined;

function scheduleInspect(): void {
  if (!debugOn) return;
  clearTimeout(inspectTimer);
  inspectTimer = setTimeout(() => void refreshInspect(), 250);
}

async function refreshInspect(): Promise<void> {
  try {
    const tree = await invoke<InspectNode[]>("inspect_document", { markdown: getMarkdown() });
    debugEl.replaceChildren(renderNodes(tree));
  } catch (e) {
    debugEl.textContent = String(e);
  }
}

function renderNodes(nodes: InspectNode[]): DocumentFragment {
  const frag = document.createDocumentFragment();
  for (const n of nodes) {
    const d = document.createElement("details");
    d.open = true;
    const s = document.createElement("summary");
    const hash = (n.hash || "").replace("sha256:", "").slice(0, 8);
    const span = (cls: string, text: string) => {
      const el = document.createElement("span");
      el.className = cls;
      el.textContent = text;
      return el;
    };
    s.append(span("ty", n.type), span("path", n.path), span("hash", hash));
    d.appendChild(s);
    const pre = document.createElement("pre");
    pre.className = "nodejson";
    pre.textContent = JSON.stringify(n.node, null, 2);
    d.appendChild(pre);
    if (n.children?.length) {
      const kids = document.createElement("div");
      kids.className = "kids";
      kids.appendChild(renderNodes(n.children));
      d.appendChild(kids);
    }
    frag.appendChild(d);
  }
  return frag;
}

document.querySelector("#debug-toggle")!.addEventListener("click", () => {
  debugOn = !debugOn;
  document.body.classList.toggle("debug", debugOn);
  if (debugOn) void refreshInspect();
});

updateWindowTitle();
