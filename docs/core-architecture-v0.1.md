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
| `schema` | The base-schema type descriptors + the document `types` registry (format §6.2–§6.4); resolves a type's content model and child rules. The engine is **schema-driven** — `validate`, `render`, and `edit` consult it rather than matching a fixed type enum. |
| `id` | `doc_id`/`node_id`/`asset_id` generation; `krg://` ref parse/format. |
| `hash` | RFC 8785 canonical JSON + SHA-256; `Rev` derivation (12-hex truncation). |
| `render` | Schema-driven projection: rich render for known types, **generic structural render by content model** for declared custom types (read side); Karanga Markdown → `Run`/`Mark`/children (write side). |
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
    pub ty: TypeName,            // base ("heading", "table") or namespaced custom ("acme:callout")
    pub content: NodeContent,    // Inline(Vec<Run>) | Raw(String) | Empty  — kind set by the type's descriptor
    pub attrs: Map,
    pub ext: Map,                // the `x` bag, preserved verbatim
}
// Node shape is validated against a TypeDescriptor (schema module), not a fixed enum:
pub struct TypeDescriptor { pub content: ContentModel, pub children: Option<ChildRule>, pub attrs: AttrSchema }
// ContentModel = Empty | Inline | Raw ;  children present ⇒ container (spine holds them)

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

**Reads** may use either store directly. **Writes** always go through a `DirStore` (a working
copy); a packed `.krg` is never mutated in place. See §4.1.

### 4.1 Working copies & the session model (C-a, resolved)

All writes operate on a **working copy** (an exploded `DirStore`). One mechanism, two lifetimes:

- **Ephemeral session (one-shot).** A single CLI/agent write opens a working copy, applies the
  change, repacks, and tears down. Stateless from the caller's view.
- **Live session (interactive / shared).** The editor — or an agent doing many edits — opens a
  working copy that persists. Edits are per-node atomic writes on the exploded parts; **repack
  happens on save/close + an autosave interval, not per edit.** This is the only model that
  makes WYSIWYG editing and concurrent human+agent writing tractable: explode-per-edit would
  repack the whole archive on every keystroke and would lose writes when two repacks race.

**Store resolution.** For any operation the engine resolves the active store for a document: a
live working copy if one is open (it is authoritative), otherwise the packed `.krg` (`ZipStore`
for reads; an ephemeral working copy for writes). A read never returns stale content while a
session is open.

**Coordination is filesystem-based — no daemon.** A live working copy is discoverable and
guarded by a lock/owner marker; concurrent writers (editor + agent) share the one working copy,
serialize per node via the §6 CAS, and watch the directory to reflect each other's edits live.
Repack is guarded by the brief advisory lock. The shared working directory *is* the
coordination substrate FR-21 assumes ("writers share a filesystem") — consistent with the
no-hub/no-daemon decision.

**Location.** A live working copy lives in a cache directory keyed by `doc_id`
(`${cache}/karanga/work/<doc_id>/`), found deterministically by reading the target file's
uncompressed manifest. Keeps the user's folder clean (just the `.krg`); needs no collection or
marker concept. *(Resolved: cache-dir keyed by `doc_id`, over a co-located `.work/` sidecar —
chosen to avoid cluttering and `.gitignore`-polluting the user's folders.)*

**Staleness & recovery.** While a live session is open the packed `.krg` is stale until save;
the autosave interval bounds this (same tradeoff as `.docx`). A working copy whose owner is
dead (crash) is detected on next open and recovered — repacked if it has unsaved changes, else
discarded.

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
- **Authoring inverse:** `insert_node`/`update_node` accept content in **Karanga Markdown**
  (the CommonMark subset, interface spec §8); `render` parses it into `Run`/`Mark`. Render and
  parse are semantic inverses, so the read projection round-trips (C-e, resolved).

## 7. Search & scope

- **`get_node`/`get_outline`/`find_nodes`** are single-document and need no scope.
- **`find_documents`/`search`/cross-doc `get_links`** take a `Scope` (directory). `scope`
  enumerates `.krg` files and, for Tier-1, reads their (uncompressed) manifests — which, being
  `STORE`d, are also reachable by external `grep`/`rg`. No corpus/vault object exists.
- **`search` (content):** backed by **Tantivy in v0.1** (C-b, resolved) for ranked/fuzzy
  results, built as a **rebuildable, path-scoped cache index** — a pure accelerator, never
  authoritative, regenerable by scanning the scoped files. (Kept behind a Cargo feature so
  minimal/embedded builds can drop it, but **enabled by default**.) A plain scan remains the
  fallback when the index is absent.

## 8. Cross-cutting

- **Sync core (C-c, resolved).** `krg-core` is synchronous. Local file I/O gains nothing from
  async (OS file reads are not truly async; tokio's `fs` uses a blocking pool regardless), and
  sync keeps the library lean and cleanly bindable (WASM is single-threaded; async over
  FFI/WASM is painful). Consumers adapt trivially:
  - `krg-mcp`, if it runs on an async runtime, wraps calls in `spawn_blocking`.
  - The CLI is naturally synchronous.
  - Internal parallelism (scanning many files for `find_documents`/`search`) uses **data
    parallelism (`rayon`)**, not async I/O — the right tool for a CPU/disk-bound fan-out.
- **Errors vs outcomes.** `Result<_, Error>` for hard failures (IO, corrupt archive, schema
  invalid). `Stale` is a normal `WriteOut` variant, not an `Error`.
- **Validation.** `validate` runs the format §11 checks; `edit` runs a cheap subset
  (referential integrity of spine/links) on every commit, full validation on demand.

## 9. Dependencies (initial)

| Need | Crate | Notes |
|---|---|---|
| ZIP | `zip` | Random-access read, `STORE`/`DEFLATE` control for the compression policy. |
| JSON | `serde` + `serde_json` | Model (de)serialization. |
| Canonical JSON | **vendored (~30 LOC, no dep)** (C-d, resolved) | Sorted keys + compact + UTF-8 over the no-float / ASCII-key domain (format §9.1); byte-identical to RFC 8785 for that domain. |
| Hash | `sha2` | SHA-256. |
| IDs | `uuid` (+ `ulid`?) | `doc_id` UUIDv4; node ids ULID/base32. |
| Markdown | `pulldown-cmark` (parse, **GFM tables on**) + small renderer | Karanga Markdown subset incl. GFM tables, nested lists, and the `:::` directive for custom block types (interface §8). |
| Full-text (opt) | `tantivy` | Feature-gated; §7. |

## 10. Open questions

- ~~**C-a.** Working-copy model — *resolved:* session model, persistent live working copy for
  interactive/shared editing, ephemeral for one-shot; filesystem-coordinated, no daemon (§4.1).
  Sub-decision still open: working-copy location (cache-dir vs co-located sidecar).~~
- ~~**C-b.** Tantivy index — *resolved:* ship in v0.1, default-on, as a rebuildable path-scoped
  cache (§7).~~
- ~~**C-c.** Core concurrency — *resolved:* sync core; consumers use `spawn_blocking`/`rayon`
  (§8).~~
- ~~**C-d.** Canonical JSON — *resolved:* vendored minimal canonicalizer over the no-float
  domain (§9 / format §9.1).~~
- ~~**C-e.** Authoring/projection dialect — *resolved:* Karanga Markdown, a CommonMark subset
  with `krg://`-href refs (interface spec §8).~~

All sub-decisions are now resolved: working-copy location → cache-dir keyed by `doc_id`
(§4.1); multi-block `update_node` on a non-container → error (interface §8.3); out-of-dialect
authoring input → reject with a diagnostic (interface §8.4).
