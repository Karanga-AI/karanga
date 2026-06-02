# Karanga Operation Interface Specification — v0.1 (Draft)

**Status:** Design — not yet stable
**Versioning:** This interface is a versioned contract (independent semver), alongside the
on-disk format spec. `spec/format-v0.1.md` defines the `.krg` file a third party reads;
*this* document defines the **operations** Karanga's engine (`krg-core`) exposes and that
`krg-cli` and `krg-mcp` surface to humans and agents.

> Decisions still open are tagged **[D]**.

---

## 1. Scope & principles

The on-disk representation is JSON, but the agent never sees it. Every operation returns a
*projection* — a token-lean rendering — never raw parts.

- **Intent-shaped verbs, no query language.** The agent calls named operations; it never
  authors SQL, JSONPath, or any query expression. *(FR-25)*
- **Cheapest-first, tiered.** Read verbs map to the three tiers; descriptions steer the agent
  discover → outline → node, paying context only for the rung it is on. *(FR-26)*
- **No layout knowledge required.** The agent never needs to understand the `.krg` package
  structure. *(FR-27)*
- **Projections, not serializations.** Outputs are Markdown/plain text; hashes and run/mark
  internals are stripped. *(C1)*
- **Opaque revision tokens.** Reads return a short `rev` per node; writes accept it for
  optimistic concurrency. The agent never sees the full content hash.

**These are engine/application operations, not format operations.** A single `.krg` is a
standalone file that knows nothing about other files — exactly like `.docx`. Operations that
span more than one document therefore take an explicit **scope** (a directory path); Karanga
has **no "vault" / collection / corpus concept** baked into the format or the engine. See §6.

## 2. References, revisions, and scope

- A **ref** is a `krg://` URI (format §3.1). Verbs accept and return refs; the agent treats
  them as opaque handles.
- A **rev** is a node's revision token: the first 12 hex characters of its content hash
  (format §9). It is always used together with a specific `ref`, so truncation collisions
  across different nodes are immaterial. The agent passes it back on a write to assert "I'm
  editing the version I read"; it is never parsed.
- A **scope** is a filesystem path (a file or, for cross-document operations, a directory
  searched recursively). It defaults to the caller's working directory or a configured root.
  Scope is a plain path — not a registry or managed workspace.

## 3. Read operations

### 3.1 `find_documents` — Tier 1 (discovery)

```
find_documents(query: string, scope?: path, limit?: int = 10)
  → [ { ref, title, description } ]
```

Matches document `title` + `description` across the `.krg` files in `scope` (manifest-level;
no node bodies read). Implemented over standard search where possible (§6). Projection:

```
1. Retry Policy — How the gateway retries upstream failures.   krg://9f1c…/
2. Rate Limiting — Token-bucket limits per client.             krg://2b7d…/
```

### 3.2 `get_outline` — Tier 2 (index)

```
get_outline(doc: ref)
  → indented headings; labels + refs; no bodies, no hashes
```

Answerable from `spine.json` alone. Projection:

```
Retry Policy   krg://9f1c…/
- Introduction            ⟨h_intro⟩
- Methods                 ⟨h_methods⟩
  - Results               ⟨h_results⟩
- Limitations             ⟨h_limits⟩
```

### 3.3 `get_node` — Tier 3 (content)

```
get_node(node: ref)
  → { ref, type, rev, content }   // content = rendered Markdown/plain text of one node
```

Extracts a single node part and renders it. The agent receives prose, not structure; `rev`
enables a later CAS write.

### 3.4 `get_section` — a section subtree

```
get_section(heading: ref)
  → rendered Markdown of the heading and its entire subtree
```

For "give me the whole Methods section." No size cap — the agent chooses deliberately.

### 3.5 `find_nodes` — filter by segment type

```
find_nodes(doc: ref, type?: string)
  → [ { ref, type, label? } ]
```

E.g. "all blockquotes in this document." Answerable from `spine.json` (the `type` projection)
with **no body reads**; the agent fetches full content via `get_node` for the ones it wants.
*(FR-12)*

### 3.6 `search` — full-text / fuzzy across documents

```
search(query: string, scope?: path, fuzzy?: bool = true)
  → [ { doc, node, snippet } ]
```

Content search across the `.krg` files in `scope`, returning matching nodes with a snippet.
Backed by standard search tooling and an optional cache index (§6); cross-document by virtue
of the `scope` path, not a corpus concept.

### 3.7 `get_links` — traverse the graph

```
get_links(node: ref, direction?: "out" | "in" | "both" = "out", scope?: path)
  → [ { from, to, type } ]
```

Outgoing (this node's references), incoming (backlinks), or both. Outgoing reads the document's
`links.json`; incoming/backlinks across documents are resolved within `scope`. *(FR-13)*

## 4. Write operations

Read and write ship together in v0.1. All writes operate through the engine on the exploded
form and repack on commit (format §2.1). No lock is held across calls; concurrency is
optimistic (§5).

```
create_document(title: string, description?: string, at?: path)
  → { ref }

insert_node(doc: ref, node: { type, content, attrs? }, at: position)
  → { ref, rev }
      // position = { parent?: ref, after?: ref } | { parent?: ref, index?: int }
      // content is sent as Markdown/plain text; the engine parses it into runs/marks.

update_node(node: ref, change: { content?, attrs? }, rev: token)
  → { ref, rev }  |  { conflict: "stale", current_rev, current: <rendered> }

move_node(node: ref, to: position, rev?: token)   → { ref }
delete_node(node: ref, rev?: token)               → { ok }

set_link(from: ref, to: ref, type: string)         → { ok }
remove_link(from: ref, to: ref, type: string)      → { ok }

add_media(doc: ref, { media_kind, source, alt?, caption? }, at: position)
  → { ref }
```

- **Authoring is text-first.** Agents send Markdown-ish content; the engine structures it into
  the node model (the inverse of the read projection). Humans, via the editor, never touch any
  of this — they edit the rendered document. *(C2, C5)*
- **Structural edits** (`insert_node` with a `parent`, `move_node`, `delete_node` of a
  container) update the spine tree.

## 5. Concurrency contract (optimistic, serverless)

Surfacing format §9 and REQUIREMENTS FR-20…FR-24:

- A read that may precede a write returns the node's `rev`.
- `update_node` (and optionally `move`/`delete`) takes that `rev`. The engine commits via
  compare-and-swap: if the on-disk node still matches `rev`, the write applies and a new `rev`
  is returned; otherwise it returns `{ conflict: "stale", current_rev, current }` and the agent
  re-reads and retries. No lease, heartbeat, or daemon. *(FR-23)*
- Omitting `rev` on a write is a **last-writer-wins** commit; the engine MAY warn. **[D]**
- Edits to different nodes never conflict (FR-22); only same-node clashes surface here.

## 6. Cross-file search & discovery — the file-format model

Karanga follows how standalone file formats actually handle "many files," rather than
inventing a collection/index of its own:

- **The filesystem is the collection.** Folders are the only grouping. Cross-document
  operations take a directory `scope`; there is no vault, registry, or marker directory.
- **Standard tools do cross-file search.** `grep`/`rg`/`fd` and the OS indexers
  (Spotlight, Windows Search) are the primary mechanism. To make this work on packed `.krg`
  files, the format provides grep-ability provisions — an uncompressed `manifest.json`, an
  optional uncompressed full-text part, and a title-bearing filename convention (to be
  finalized in `format-v0.1.md`; pending the compression-default decision). **[D]**
- **OS-native search via an importer plugin**, the `.docx`-faithful approach: ship a macOS
  **Spotlight importer** (`.mdimporter`) and a **Windows IFilter** so the OS indexer can read
  inside `.krg`. The index belongs to the OS, not to Karanga. *(post-v0.1 nice-to-have)* **[D]**
- **Optional engine cache index.** For ranked/fuzzy results beyond what grep gives, the engine
  MAY maintain a rebuildable cache index (e.g. Tantivy) scoped to a directory. It is a pure
  accelerator — deletable and regenerable by scanning the files — never authoritative, never a
  format artifact. Possibly unnecessary for v1 if grep + the OS index suffice. **[D]**

## 7. Surfaces

The CLI and MCP server are thin renderings of the same operations over `krg-core`.

| Operation | `krg-cli` | `krg-mcp` (MCP tool) |
|---|---|---|
| find_documents | `krg find <q> [dir]` | `find_documents` |
| get_outline | `krg outline <file>` | `get_outline` |
| get_node | `krg get <file> <id>` | `get_node` |
| get_section | `krg section <file> <id>` | `get_section` |
| find_nodes | `krg nodes <file> --type quote` | `find_nodes` |
| search | `krg search <q> [dir]` | `search` |
| get_links | `krg links <file> <id>` | `get_links` |
| writes | `krg new`/`insert`/`set`/… | `create_document`/`insert_node`/… |

- **CLI** defaults to human/pipe-friendly output and offers `--json` for raw structures (the
  dev/tooling escape hatch — the agent path never uses it).
- **MCP** returns the lean projections (§3) as tool results, with tool descriptions encoding
  the cheapest-first tiered access pattern.

## 8. Open questions

- **Q-a.** `last-writer-wins` on a missing `rev` (§5) — warn, or require `rev` on all mutating
  writes?
- **Q-b.** Ship the Spotlight importer / Windows IFilter in v0.1, or defer (§6)?
- **Q-c.** Is an engine cache index (Tantivy) needed for v1, or do grep + the OS index cover it
  until proven otherwise (§6)?
- **Q-d.** The format-level grep-ability / compression-default decision (uncompressed manifest,
  optional full-text part, filename convention) is still pending in `format-v0.1.md`.
