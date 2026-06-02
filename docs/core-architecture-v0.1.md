# `krg-core` Architecture — v0.1 (Draft)

**Status:** Design — not yet stable
**Implements:** `spec/format-v0.1.md` (the `.krg` file) and `spec/interface-v0.1.md` (the
operations). This document is reference-implementation design, not a stable contract.

> Decisions still open are tagged **[D]**.

---

## 1. Boundaries

`krg-core` is the one Rust library that knows the format and the operations. Everything else
is a thin consumer.

**In scope:** ZIP container I/O (packed + exploded), the node/spine/link model and its JSON
(de)serialization, canonical hashing + `rev`, rendering nodes/sections/documents to lean
projections, the inverse (text → node model) for authoring, the read verbs, the write verbs
with optimistic CAS, validation, and directory-scoped discovery.

**Out of scope:** MCP transport (`krg-mcp`), CLI parsing (`krg-cli`), format conversion
(`krg-convert`), the editor UI, OS importer plugins. These call `krg-core`.

```
krg-cli ─┐
krg-mcp ─┼─► krg-core ─► .krg files (packed) / working dirs (exploded)
krg-convert ─┘   (+ optional Tantivy index, feature-gated)
```

## 2. Module map

| Module | Responsibility |
|---|---|
| `container` | ZIP pack/unpack, exploded-dir I/O, compression policy (manifest `STORE`), random-access single-entry extraction, atomic part writes, the `Store` abstraction (§4). |
| `model` | Types: `Document`, `Manifest`, `Spine`, `Node`, `NodeContent`, inline `Run`/`Mark`, `Link`, `Media`. Serde to/from JSON parts. |
| `id` | `doc_id`/`node_id`/`asset_id` generation; `krg://` ref parse/format. |
| `hash` | RFC 8785 canonical JSON + SHA-256; `Rev` derivation (12-hex truncation). |
| `render` | Node/section/document → Markdown/plain-text projections (read side); Markdown → `Run`/`Mark` (write side). |
| `query` | Read verbs: `find_documents`, `get_outline`, `get_node`, `get_section`, `find_nodes`, `search`, `get_links`. |
| `edit` | Write verbs with CAS: create/insert/update/move/delete, link + media ops; spine/links projection maintenance; repack. |
| `validate` | Conformance checks (format §11). |
| `scope` | Directory-path scoping; standard-tooling-backed discovery; optional index hook. |
| `error` | `Error` (hard failures) and typed soft outcomes (e.g. `Stale`). |

## 3. Core types (sketch)

```rust
pub struct Ref(String);          // krg://<doc_id>/<node_id>  (opaque handle)
pub struct Rev(String);          // first 12 hex of a node's content hash
pub struct Scope(PathBuf);       // a file or a directory (recursive)

pub struct Node {
    pub id: NodeId,
    pub ty: NodeType,            // Heading{level}, Paragraph, Blockquote, Code{lang}, List{ordered}, ListItem, Media, Divider, Ext(String)
    pub content: NodeContent,    // Inline(Vec<Run>) | Code(String) | MediaRef(..) | Empty
    pub attrs: Map,
    pub ext: Map,                // the `x` bag, preserved verbatim
}

pub struct Run { pub text: String, pub marks: Vec<MarkToken> }   // MarkToken = Simple(&str) | Key(String)

pub enum ReadOut {              // what query returns — already projected, never raw JSON
    Docs(Vec<DocHit>),          // { ref, title, description }
    Outline(OutlineTree),       // headings: label + ref + nesting
    Node { r: Ref, ty: NodeType, rev: Rev, content: String },   // rendered
    Section(String),            // rendered Markdown
    Nodes(Vec<NodeHit>),        // { ref, ty, label? }
    Hits(Vec<SearchHit>),       // { doc, node, snippet }
    Links(Vec<Link>),
}

pub enum WriteOut { Created(Ref), Updated { r: Ref, rev: Rev }, Ok, Stale { current_rev: Rev, current: String } }
```

`Stale` is a **return value, not an error** — callers (agents) handle it by re-reading.

## 4. The `Store` abstraction — packed vs exploded

The packed/exploded duality (format §2.1) is hidden behind one trait so `query`/`render`/`edit`
work against either form:

```rust
pub trait Store {
    fn read_part(&self, path: &str) -> Result<Bytes>;        // random-access single entry
    fn list(&self, prefix: &str) -> Result<Vec<String>>;
    fn write_part(&mut self, path: &str, bytes: &[u8]) -> Result<()>;  // atomic
    fn remove_part(&mut self, path: &str) -> Result<()>;
}
```

- **`ZipStore`** — opens a packed `.krg`, uses the ZIP **central directory** to extract a
  single entry without inflating the whole archive (this is what makes `get_node` cheap and
  satisfies "extract one node without parsing others"). Read-optimized.
- **`DirStore`** — an exploded working directory. Read + write; the form edits run against.

**Reads** may use either store directly. **Writes** require a `DirStore`: given a packed file,
`edit` explodes it into a working directory (a temp dir or a sidecar `.krg.work/`), mutates,
then repacks. **[D: explode-to-temp-and-repack-per-session vs. keep a persistent exploded
working copy while a document is "open".]**

## 5. Read & render flow

```
Ref ─► resolve to (Store, part path)
    ─► read_part (lazy, single entry)
    ─► deserialize (model)
    ─► render/project (render)  ─► lean text out (ReadOut)
```

- `get_outline` / `find_nodes` read **only `spine.json`** (the denormalized `type`/`label`
  projection) — no node bodies.
- `get_node` reads one node part; `get_section` reads a heading's subtree parts in spine order.
- Rendering strips structure: inline `Run`/`Mark` → Markdown; the agent sees prose. Hashes are
  never emitted; `rev` is surfaced only where a follow-up write needs it.

## 6. Write flow & CAS

```
write verb ─► ensure DirStore (explode if packed)
           ─► [mutating existing node] read current node, hash it, compare to supplied Rev
                 ├─ mismatch ─► return Stale { current_rev, current(rendered) }   (no write)
                 └─ match ─► write node part atomically (temp + fsync + rename)
           ─► update spine projection (type/hash/label) and links.json as needed
           ─► repack to .krg (under a brief advisory lock)
           ─► return new Rev
```

- **Rev requirement (Q-a, resolved):** `update_node`, `move_node`, `delete_node` **require**
  `rev` (they touch an existing node's state); `create_document`, `insert_node`, `add_media`
  do not; `set_link`/`remove_link` are idempotent and do not. There is no last-writer-wins
  path.
- **Atomic part write:** temp file + `fsync` + atomic `rename` on the exploded dir. The CAS
  check (re-hash current vs supplied `rev`) happens immediately before the rename.
- **Projection upkeep:** every node write updates that node's spine entry (`type`/`hash`/
  `label`) so the index stays consistent (format §5.2). Inline `ref` marks are mirrored into
  `links.json` (format §7.2/§8.3).
- **Authoring inverse:** `insert_node`/`update_node` accept Markdown-ish content; `render`
  parses it into `Run`/`Mark`. Round-trips with the read projection.

## 7. Search & scope

- **`get_node`/`get_outline`/`find_nodes`** are single-document and need no scope.
- **`find_documents`/`search`/cross-doc `get_links`** take a `Scope` (directory). `scope`
  enumerates `.krg` files and, for Tier-1, reads their (uncompressed) manifests — which, being
  `STORE`d, are also reachable by external `grep`/`rg`. No corpus/vault object exists.
- **`search` (content):** v0.1 default scans node bodies across scoped files (acceptable at
  local scale). A `tantivy` **feature flag** adds a rebuildable, path-scoped cache index for
  ranked/fuzzy results — pure accelerator, never authoritative. **[D: ship the index in v0.1
  or rely on scanning + the OS index?]**

## 8. Cross-cutting

- **Sync core.** `krg-core` is synchronous; file I/O at local scale doesn't need async. `krg-mcp`
  wraps calls on a threadpool if it needs concurrency; the CLI is naturally sync. **[D]**
- **Errors vs outcomes.** `Result<_, Error>` for hard failures (IO, corrupt archive, schema
  invalid). `Stale` is a normal `WriteOut` variant, not an `Error`.
- **Validation.** `validate` runs the format §11 checks; `edit` runs a cheap subset
  (referential integrity of spine/links) on every commit, full validation on demand.

## 9. Dependencies (initial)

| Need | Crate | Notes |
|---|---|---|
| ZIP | `zip` | Random-access read, `STORE`/`DEFLATE` control for the compression policy. |
| JSON | `serde` + `serde_json` | Model (de)serialization. |
| Canonical JSON | JCS impl or in-house | RFC 8785 for the hash input (format §9, Appendix A5). **[D]** |
| Hash | `sha2` | SHA-256. |
| IDs | `uuid` (+ `ulid`?) | `doc_id` UUIDv4; node ids ULID/base32. |
| Markdown | `pulldown-cmark` (parse) + small renderer | Read projection out, authoring parse in. |
| Full-text (opt) | `tantivy` | Feature-gated; §7. |

## 10. Open questions

- **C-a.** Write working-copy model: explode-to-temp per write vs. a persistent exploded
  "open document" the editor and engine share (§4).
- **C-b.** Ship the `tantivy` index in v0.1, or scanning + OS index only (§7)?
- **C-c.** Sync-only core, or expose async entry points for `krg-mcp` (§8)?
- **C-d.** Canonical-JSON: pull an RFC 8785 crate or vendor a minimal JCS (§9 / format A5)?
- **C-e.** Markdown dialect for the authoring/round-trip projection — CommonMark subset, and
  how marks/`ref`s map to it.
