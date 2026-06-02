# Karanga Document Format Specification

**Version:** 0.1 (Draft)
**Status:** Design — not yet stable
**File extension:** `.krg`
**Media type:** `application/vnd.karanga.document+zip`

This document specifies the on-disk structure of a Karanga document (`.krg`). It is the
contract between implementations; any conforming reader or writer must behave as described
here. Requirement language ("MUST", "SHALL", "SHOULD", "MAY") follows RFC 2119.

> **Draft decisions.** Sections marked **[D]** record a design decision that is open to
> revision during the design phase. They are noted inline so they can be revisited without
> re-reading the whole document.

---

## 1. Overview

A `.krg` file is a **package** (a ZIP archive) whose entries are small, independently
addressable parts. A document is decomposed into typed **atomic nodes**; an ordered tree
(the **spine**) reconstructs the document from those nodes. The package structure doubles
as a query index: document discovery reads only the manifest, the outline reads only the
spine, and a single node is fetched by extracting one part.

```
mydoc.krg
├── mimetype          # "application/vnd.karanga.document+zip" (stored, uncompressed)
├── manifest.json     # document metadata (TIER 1: discovery)
├── spine.json        # ordered tree of node refs + index projection (TIER 2: outline)
├── nodes/
│   └── <node_id>.json  # one atomic node per part           (TIER 3: content)
├── links.json        # the typed link graph
└── media/
    └── <asset_id>.<ext>  # embedded media (when media_mode = embedded)
```

These properties are normative consequences of the layout:

- A reader **MUST** be able to extract any single node without parsing other node parts.
- Discovery (Tier 1) **MUST** be answerable from `manifest.json` alone.
- Outline and node-type queries (Tier 2) **MUST** be answerable from `spine.json` alone.

## 2. Container

- A `.krg` file **MUST** be a valid ZIP archive.
- The first entry **MUST** be a file named `mimetype`, **stored without compression**,
  whose content is exactly `application/vnd.karanga.document+zip` with no trailing newline.
  This enables magic-byte file-type detection. *(Pattern borrowed from EPUB/OCF.)*
- `manifest.json` **MUST** be stored without compression (`STORE`). This keeps the document
  `title` and `description` as plain bytes inside the package, so standard filesystem search
  (`grep`/`ripgrep`, OS indexers) can perform Tier-1 discovery on packed `.krg` files without a
  Karanga reader. The manifest is tiny, so the cost is negligible.
- All remaining entries (`spine.json`, `links.json`, node parts, media) **MAY** be compressed
  (DEFLATE).
- *Compression policy is a two-way door.* v0.1 is deliberately **lean**: titles and
  descriptions are discoverable by standard tooling, node bodies stay compressed. A future
  minor version MAY add an optional, uncompressed full-text part to make node **content**
  grep-able (Appendix A6); because unknown entries are preserved on round-trip, that addition
  is backward-compatible.
- Entry paths **MUST** be UTF-8, use `/` as separator, and **MUST NOT** contain `..` or
  absolute paths.
- The reserved top-level paths are `mimetype`, `manifest.json`, `spine.json`, `links.json`,
  the `nodes/` directory, and the `media/` directory. Implementations **MUST** preserve
  unknown additional entries on round-trip (forward compatibility).

### 2.1 Packed and exploded forms

A `.krg` is the **packed** form (a single file, for storage and exchange). While editing, an
implementation **MAY** operate on an **exploded** form — a directory containing the same
parts — and repack on save. The two forms are byte-equivalent in their part contents. The
exploded form is an implementation detail and is **not** part of the interchange contract.

### 2.2 Filename convention

A `.krg` file **SHOULD** be named with a sanitized form of its title followed by a short
`doc_id` suffix, e.g. `Retry Policy [9f1c2e4a].krg`. This makes documents discoverable by
title through the most basic tools (`ls`, `fd`, `fzf`, GUI file managers, OS filename search)
with no reader involved.

- The filename is a **convenience, not authoritative**. `manifest.doc_id` is the source of
  truth; renaming a file (e.g. after a title change) **MUST NOT** affect document identity or
  any `krg://` reference.
- Implementations **MAY** rename the file when the title changes but **MUST NOT** rely on the
  filename for correctness.

## 3. Identifiers

- **`doc_id`** — a document's globally unique identifier. **MUST** be a UUID (v4 recommended).
  Stable for the life of the document.
- **`node_id`** — a node's identifier, unique **within its document**. An opaque string
  matching `^[A-Za-z0-9_-]{1,64}$`. Writers **SHOULD** generate collision-resistant values
  (e.g. a ULID or ≥10-character random base32). A `node_id` **MUST NOT** be reused within a
  document once assigned, even after the node is deleted.
- **`asset_id`** — a media asset's identifier, unique within its document; same character
  rules as `node_id`.

### 3.1 References

A reference to any node, in any document, is a URI:

```
krg://<doc_id>/<node_id>     # a specific node
krg://<doc_id>               # a document (its root)
```

Intra-document references **MAY** use the short form `krg:///<node_id>` (empty authority),
which resolves against the containing document's `doc_id`.

## 4. `manifest.json`

Document-level metadata. Read alone, it answers Tier-1 discovery.

```json
{
  "krg": "0.1",
  "doc_id": "9f1c2e4a-6b2d-4f8a-9c3e-1a2b3c4d5e6f",
  "title": "Retry Policy",
  "description": "How the gateway retries upstream failures.",
  "created": "2026-06-01T12:00:00Z",
  "modified": "2026-06-01T12:34:56Z",
  "media_mode": "embedded",
  "authors": [{ "name": "Cameron G. Gould" }],
  "x": {}
}
```

| Field | Req. | Type | Notes |
|---|---|---|---|
| `krg` | MUST | string | Spec version `MAJOR.MINOR` the file conforms to. |
| `doc_id` | MUST | string | UUID. |
| `title` | MUST | string | Human title; primary Tier-1 search field. |
| `description` | SHOULD | string | Short abstract; secondary Tier-1 search field. |
| `created` | SHOULD | string | RFC 3339 timestamp. |
| `modified` | SHOULD | string | RFC 3339 timestamp. |
| `media_mode` | MUST | string | `"embedded"` or `"referenced"` (§8). |
| `authors` | MAY | array | Each `{ "name": string, "x"?: object }`. |
| `x` | MAY | object | Namespaced extension bag (§10). |

## 5. `spine.json` — structure and index

The spine is the document's **ordered tree** of node references. Reading order is the
**pre-order depth-first traversal** of the tree. The spine is also the index: each entry
carries a lightweight projection of its node so Tiers 1–2 and type filtering need no body
reads.

```json
{
  "root": [
    {
      "id": "h_intro",
      "type": "heading",
      "hash": "sha256:3a7b…",
      "label": "Introduction",
      "children": [
        { "id": "p_1", "type": "paragraph", "hash": "sha256:91c0…" },
        { "id": "q_1", "type": "blockquote", "hash": "sha256:5d2e…" }
      ]
    },
    {
      "id": "h_methods",
      "type": "heading",
      "hash": "sha256:88af…",
      "label": "Methods",
      "children": [
        { "id": "p_2", "type": "paragraph", "hash": "sha256:0b13…" },
        {
          "id": "h_results",
          "type": "heading",
          "hash": "sha256:77de…",
          "label": "Results",
          "children": [
            { "id": "c_1", "type": "code", "hash": "sha256:aa01…" }
          ]
        }
      ]
    }
  ]
}
```

### 5.1 Spine entry

| Field | Req. | Type | Notes |
|---|---|---|---|
| `id` | MUST | string | `node_id` of the referenced node. |
| `type` | MUST | string | Node type (cached from the node; §6). Enables type filtering from the spine. |
| `hash` | MUST | string | Content hash of the node part (§9). Integrity + concurrency token. |
| `label` | MAY | string | Short plain-text label, present for nodes that have one (e.g. heading text). Powers the outline without reading bodies. |
| `children` | MAY | array | Ordered child spine entries. Presence makes the node a **container**. |

### 5.2 Structural rules

- Every `id` in the spine **MUST** correspond to exactly one `nodes/<id>.json` part, and
  every node part **MUST** appear exactly once in the spine. (Orphan nodes and dangling
  references are invalid.)
- The tree **MUST NOT** contain cycles, and a node **MUST NOT** appear more than once.
- `type`, `hash`, and `label` are a **denormalized projection** of the node. Writers **MUST**
  keep them consistent with the node part on every write. *(Deliberate index denormalization
  — the cost of single-read Tier-1/2. Readers treat the node part as the source of truth if
  they ever disagree, and **SHOULD** flag the document as needing reindex.)* **[D]**

### 5.3 Sections

A **section** is a `heading` node together with its `children` subtree. There is no separate
"section" node type; the heading *is* the section's root and label. **[D]**

- Content appearing before any heading is held as direct children of `root` (a preamble).
- Heading **level** (`attrs.level`, §6.2) is a presentation attribute and is independent of
  tree depth, though writers **SHOULD** keep them aligned.
- "Get the outline" = walk the spine emitting heading entries (`type == "heading"`) with
  their `label` and nesting. "Get a section" = return a heading entry's subtree.

## 6. Nodes

Each node is a part at `nodes/<node_id>.json`. The envelope is common to all types; the
shape of `content` and `attrs` is type-specific.

### 6.1 Envelope

```json
{
  "id": "p_1",
  "type": "paragraph",
  "content": [ /* inline runs, a string, or a typed payload */ ],
  "attrs": {},
  "x": {}
}
```

| Field | Req. | Type | Notes |
|---|---|---|---|
| `id` | MUST | string | Matches the part filename and the spine entry. |
| `type` | MUST | string | A core type (§6.2) or a namespaced extension type (§10). |
| `content` | per type | varies | See each type. |
| `attrs` | MAY | object | Type-specific attributes. |
| `x` | MAY | object | Namespaced extension bag. |

### 6.2 Core node types (v0.1)

| `type` | `content` | `attrs` | Container? |
|---|---|---|---|
| `heading` | inline runs (§7) | `{ "level": 1–6 }` | **Yes** (owns its section) |
| `paragraph` | inline runs | — | No |
| `blockquote` | inline runs | — | No (v0.1) |
| `code` | string (raw code) | `{ "language"?: string }` | No |
| `list` | — | `{ "ordered": bool }` | **Yes** (children are `list-item`) |
| `list-item` | inline runs | — | **Yes** (MAY contain block children) |
| `media` | — | `{ "media_kind", "asset"|"src", "alt"?, "caption"? }` (§8) | No |
| `divider` | — | — | No |

- A **container** node is one whose `children` appear in the spine (§5). Its node part does
  not duplicate its children; the spine holds the structure.
- Readers encountering an **unknown** `type` **MUST** preserve it on round-trip and **SHOULD**
  render a visible placeholder rather than dropping content (§10).

## 7. Inline content model

Text-bearing nodes (`heading`, `paragraph`, `blockquote`, `list-item`) carry `content` as an
ordered array of **runs**. **[D]**

```json
"content": [
  { "text": "Retries are capped at " },
  { "text": "three attempts", "marks": ["strong"] },
  { "text": ". See ", "marks": [] },
  { "text": "the gateway doc", "marks": ["lnk1"] },
  { "text": "." }
]
```

- A **run** is `{ "text": string, "marks"?: string[] }`.
- A mark token is either a **simple mark** keyword or a key into the node's `marks` table for
  **parametric marks**.
- The **plain text** of a node is the concatenation of its runs' `text`, in order. Agents
  reading for content **SHOULD** use this projection — it is markup-free prose.

### 7.1 Simple marks (v0.1)

`strong`, `em`, `code`, `strike`. Carried as bare keywords in a run's `marks`.

### 7.2 Parametric marks

Marks that need attributes (e.g. links) are defined in a node-local `marks` table and
referenced by key:

```json
{
  "id": "p_3",
  "type": "paragraph",
  "marks": {
    "lnk1": { "type": "link", "href": "https://example.com/gateway" },
    "ref1": { "type": "ref",  "target": "krg://9f1c…/h_methods" }
  },
  "content": [
    { "text": "an external link", "marks": ["lnk1"] },
    { "text": " and an internal one", "marks": ["ref1"] }
  ]
}
```

- `link` — `{ "type": "link", "href": string }`.
- `ref` — `{ "type": "ref", "target": <krg:// reference> }`. An inline reference to another
  node/document. Every `ref` mark **MUST** have a corresponding entry in `links.json` (§8.3);
  the inline mark is for rendering, the link record is for the queryable graph. **[D]**

## 8. Media

Media is referenced by nodes of `type: "media"`; the client renders it. `media_mode` in the
manifest declares how assets are stored.

```json
{ "id": "m_1", "type": "media",
  "attrs": { "media_kind": "image", "asset": "fig1", "alt": "Latency graph", "caption": "p95 latency over time" } }
```

- `media_kind` — one of `image`, `video`, `audio`, `file`.
- When `media_mode` is `"embedded"`: `attrs.asset` is an `asset_id` resolving to
  `media/<asset_id>.<ext>` inside the package (fully self-contained document).
- When `media_mode` is `"referenced"`: `attrs.src` is an absolute URL or `file://` URI; the
  package carries no bytes.
- A node **MUST** use exactly one of `asset` (embedded) or `src` (referenced), matching the
  document's `media_mode`.
- The format never interprets media bytes; rendering is the client's responsibility.

## 8.3 `links.json` — the link graph

A flat, queryable record of typed, directed links, so the graph can be traversed without
scanning prose.

```json
{
  "links": [
    { "from": "p_3", "to": "krg://9f1c…/h_methods", "type": "ref" },
    { "from": "h_intro", "to": "krg://2b7d…/", "type": "relates" }
  ]
}
```

| Field | Req. | Type | Notes |
|---|---|---|---|
| `from` | MUST | string | `node_id` of the source node (in this document). |
| `to` | MUST | string | A `krg://` reference (intra- or inter-document). |
| `type` | MUST | string | Free vocabulary (`ref`, `cites`, `relates`, …). |

- An intra-document `to` **MUST** resolve to an existing node; an inter-document `to` **MAY**
  dangle (the target document may be absent).
- Every inline `ref` mark (§7.2) **MUST** be reflected here; the reverse is not required
  (links may exist without inline marks).

## 9. Content hashing & integrity

Each node's `hash` (stored in its spine entry) is the integrity check and the optimistic
concurrency token (it is *not* stored inside the node part, to avoid self-reference).

- **Algorithm:** SHA-256 over the **canonical serialization** of the node part, formatted as
  `"sha256:" + lowercase-hex`.
- **Canonical serialization:** UTF-8 JSON with object keys sorted lexicographically by
  Unicode code point, no insignificant whitespace, and numbers in shortest round-trip form
  (RFC 8785 / JCS is the reference). The `x` extension bag is included in the hash.
- A writer performs an update as a **compare-and-swap**: it records a node's `hash` at read
  time and, before committing, confirms the on-disk node still hashes to that value; if not,
  the write is rejected as stale and the writer re-reads (§ optimistic concurrency, see
  REQUIREMENTS FR-23). No locking is defined by the format.

## 10. Extensibility & versioning

- **Spec version** (`manifest.krg`) is `MAJOR.MINOR`. A reader **MUST** reject a document
  whose `MAJOR` it does not implement. A reader **MUST** accept a higher `MINOR` of a `MAJOR`
  it implements, ignoring unknown additions but preserving them on round-trip.
- **Extension node types and mark types** are namespaced with a vendor prefix and a colon,
  e.g. `acme:callout`. Unknown extension types **MUST** be preserved and **SHOULD** render as
  a placeholder.
- **`x` bags** (on manifest, nodes, authors, …) hold arbitrary namespaced data. Keys
  **SHOULD** be reverse-DNS or vendor-prefixed. Readers **MUST** preserve unknown `x` content.

## 11. Conformance (informative for v0.1)

A conforming `.krg` document:

1. is a valid ZIP with `mimetype` as the first, stored entry (§2);
2. has a `manifest.json` with all required fields and a supported `krg` version (§4);
3. has a `spine.json` that is a cycle-free tree in bijection with the `nodes/` parts (§5.2);
4. has spine `type`/`hash`/`label` projections consistent with the node parts (§5.2);
5. has node parts whose `content`/`attrs` are valid for their `type` (§6–§7);
6. has `links.json` whose intra-document targets resolve (§8.3);
7. resolves all media per the declared `media_mode` (§8).

A machine-readable JSON Schema set and a conformance fixture suite (sample `.krg` files with
expected outline/render/query outputs) will accompany this spec; they are deferred until the
design is ratified.

## Appendix A — open design questions

- **A1.** Spine denormalization (§5.2): accept the writer sync obligation, or normalize and
  read heading parts for the outline?
- **A2.** Should `blockquote` and `list-item` be allowed to contain block children in v0.1, or
  stay inline-only until v0.2?
- **A3.** `doc_id`/`node_id`: UUID/ULID vs. content-addressed ids.
- **A4.** Whether to admit a `table` node type in v0.1.
- **A5.** Canonical-JSON dependency (RFC 8785) — acceptable as a hard requirement for hashing?
- **A6.** Grep-friendly content (deferred, two-way door): whether/when to add an optional,
  uncompressed full-text part — and a manifest flag to toggle it — so node **content** (not
  just titles) is discoverable by standard search on packed files. Omitted from v0.1's lean
  default; backward-compatible to add later (§2).
