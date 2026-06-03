import { Editor } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import Link from "@tiptap/extension-link";
import { marked } from "marked";
import TurndownService from "turndown";
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

function title(): string {
  return titleEl.value.trim() || "Untitled";
}

function getMarkdown(): string {
  return turndown.turndown(editor.getHTML());
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
