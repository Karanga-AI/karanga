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
- **`rev` is required** for mutations of an existing node — `update_node`, `move_node`,
  `delete_node`. Creation ops (`create_document`, `insert_node`, `add_media`) need no `rev`;
  link set-ops (`set_link`/`remove_link`) are idempotent and need none. There is **no
  last-writer-wins path** — a missing required `rev` is rejected, not silently applied.
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
- **Engine cache index (Tantivy, v0.1).** `search` (§3.6) is backed by a **Tantivy** index
  scoped to a directory — rebuildable, deletable, regenerable by scanning the files, never
  authoritative, never a format artifact. grep + the OS index remain the no-engine fallback.

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

## 8. Karanga Markdown — the authoring & projection dialect

Both directions of the text interface use one dialect. The **read projection** renders nodes
to it (§3.3/§3.4), and **authoring** parses it back (`insert_node`/`update_node` content, §4).
It is a strict **profile of CommonMark**, chosen so every construct maps 1:1 to the node/mark
vocabulary — which is what makes render → parse → render **semantically lossless** (same nodes
and marks out as in; not necessarily byte-identical).

### 8.1 Block constructs

| Karanga Markdown | Node (format §6.2) |
|---|---|
| `#` … `######` (ATX, levels 1–6) | `heading` (`attrs.level`); its section = following blocks until the next same-or-higher heading |
| text block | `paragraph` |
| `> …` | `blockquote` |
| ` ```lang … ``` ` (fenced) | `code` (`attrs.language`) |
| `- ` / `* ` (bullets), `1. ` (ordered) | `list` (`attrs.ordered`) + `list-item` children |
| `---` / `***` (thematic break) | `divider` |
| `![alt](src)` as a standalone block | `media` |

- **ATX headings only** — no setext (`===`/`---`) headings; `---` is reserved for `divider`.
- **Fenced code only** — no indented code blocks (ambiguous against list/quote nesting).
- Heading-as-container sectioning (format §5.3) is derived from heading levels on parse and
  re-expressed as heading levels on render.

### 8.2 Inline constructs (within text-bearing nodes)

| Karanga Markdown | Mark (format §7) |
|---|---|
| `**text**` | `strong` |
| `*text*` | `em` |
| `` `text` `` | `code` |
| `~~text~~` | `strike` |
| `[text](https://…)` | parametric `link` (`href`) |
| `[text](krg://<doc>/<node>)` | parametric `ref` (internal link; mirrored to `links.json`) |

- **Internal references use ordinary link syntax with a `krg://` href** — there is no custom
  `[[wiki]]` syntax, so the dialect stays pure CommonMark. A link whose href parses as a
  `krg://` URI becomes a `ref`; any other href becomes a `link`.
- Inline HTML, autolinks, and reference-style (`[id]: url`) link definitions are **not** part
  of the dialect.

### 8.3 Granularity

The dialect parses at two levels: a single block fragment (e.g. `update_node` on one paragraph
or heading) and a multi-block document (`get_section`, whole-document render). On
`update_node`, content that resolves to more than one block is an error unless the target is a
container node. **[D: error vs. auto-wrap.]**

### 8.4 Conformance

- Render output MUST be parseable back to the originating nodes/marks (semantic round-trip).
- Authoring input outside this profile MUST be rejected with a diagnostic rather than silently
  coerced, so structure is deterministic. **[D: reject vs. best-effort coerce.]**
- Implementations parse with a CommonMark parser (reference: `pulldown-cmark`) restricted to the
  accepted constructs.

## 9. Open questions

- ~~**Q-a.** rev-on-writes — *resolved:* `rev` required for `update`/`move`/`delete`; no
  last-writer-wins path (§5).~~
- **Q-b.** Ship the Spotlight importer / Windows IFilter in v0.1, or defer (§6)?
- ~~**Q-c.** Engine cache index — *resolved:* Tantivy ships in v0.1, backing `search` (§6).~~
- **Q-d.** The format-level grep-ability / compression-default decision (uncompressed manifest,
  optional full-text part, filename convention) is still pending in `format-v0.1.md`.
