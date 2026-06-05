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
| `types` | MAY | object | Type-descriptor registry for the non-base node types this document uses (§6.4). Absent when the document uses only base-schema types. |
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
  they ever disagree, and **SHOULD** flag the document as needing reindex.)*

### 5.3 Sections

A **section** is a `heading` node together with its `children` subtree. There is no separate
"section" node type; the heading *is* the section's root and label.

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
  "content": "…", /* a Karanga Markdown inline string (§7), a raw string, or omitted — per the type's content model (§6.2) */
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

### 6.2 Content models and the base schema

The node-type vocabulary is **schema-driven and open**, not a closed enum. Every type is
described by a **type descriptor** (§6.3) with two independent axes: the node's own **content**
payload, and whether it is a **container** (has spine children).

The **base schema** is the set of descriptors built into v0.1. Bare type names are reserved for
the base schema; additional types are namespaced and document-declared (§6.4).

| `type` | content | children | `attrs` |
|---|---|---|---|
| `heading` | inline | `block` | `{ "level": 1–6 }` |
| `paragraph` | inline | — | — |
| `blockquote` | empty | `block` | — |
| `code` | raw | — | `{ "language"?: string }` |
| `list` | empty | `list-item` | `{ "ordered": bool }` |
| `list-item` | empty | `block` | — |
| `table` | table | — | — |
| `media` | empty | — | `{ "media_kind", "asset"\|"src", "alt"?, "caption"? }` (§8) |
| `divider` | empty | — | — |

- A **container** node is one with `children`; its children live in the spine (§5), not in the
  node part.
- **`block`** (a child shorthand) = any block-level type: `heading`, `paragraph`, `blockquote`,
  `code`, `list`, `table`, `media`, `divider`, or a custom block type (§6.4).
- **Nested lists** = a `list-item` with a `list` child; **multi-block** quotes/items = a
  `blockquote`/`list-item` with several block children. Both fall out of the generic container
  model — no special casing. A simple item or quote wraps its text in a `paragraph` child, so
  that only inline-content types carry text directly.

### 6.3 Type descriptors

A descriptor declares a type's shape:

```json
{
  "content": "empty" | "inline" | "raw",
  "children": [ "<type>", … ] | "block",
  "attrs":    { "<key>": "<value-domain>", … }
}
```

- **`content`** — the node's own payload: `empty` (none), `inline` (a canonical Karanga
  Markdown inline string, §7), `table` (a canonical GFM table serialization, §7.4), or
  `raw` (an opaque string, e.g. code).
- **`children`** — allowed child types (a list, or the `block` shorthand). Present ⇒ the type is
  a container. A type MAY be *both* inline-content and a container (e.g. `heading`: an inline
  title plus a block section).
- **`attrs`** — permitted attributes and their value domains. Values obey the no-float domain
  (§9.1): integers, strings, booleans, arrays, objects.

### 6.4 Custom node types

The vocabulary is open. A document MAY use types beyond the base schema; each such type:

- **MUST** be namespaced with a vendor prefix and colon, e.g. `acme:callout` (bare names are
  reserved for the base schema);
- **MUST** be declared in the manifest `types` registry, mapping the name to its descriptor
  (§6.3), optionally with advisory render hints:

```json
"types": {
  "acme:callout": {
    "content": "empty",
    "children": "block",
    "attrs": { "variant": "string" },
    "render": { "hint": "callout" }
  }
}
```

- A reader **MUST** validate and **structurally render** a declared custom type from its
  descriptor — a container renders its children in reading order; an inline type renders its
  text — even when it cannot render the type *richly*. A client that recognizes the type renders
  it natively.
- A type that is neither in the base schema nor declared in `types` makes the document
  **invalid**. A `.krg` therefore always carries enough to render its own structure
  (self-describing).
- `render` hints are advisory; the format never depends on them.

## 7. Inline content model

Nodes whose content model is `inline` (`heading`, `paragraph`, and any custom inline type)
carry `content` as a single string of **canonical Karanga Markdown inline syntax** (the
inline subset of the dialect, interface §8).

```json
"content": "Retries are capped at **three attempts**. See [the gateway doc](https://example.com/gateway)."
```

*(Supersedes the runs+marks array model ratified 2026-06-01: once the dialect was fixed as a
lossless round-trip (interface §8), the structured form carried no information the canonical
string doesn't — it was redundant, heavier on disk, and had an unspecified run-splitting
normalization, which made hashing ambiguous. Reversed 2026-06-04.)*

### 7.1 Inline syntax (v0.1)

- **Simple marks** — `**strong**`, `*em*`, `` `code` ``, `~~strike~~`.
- **Links** — `[text](href)`. A destination beginning `krg://` is an internal **reference**
  to another node/document; anything else is an external link. Every inline `krg://` link
  **MUST** have a corresponding entry in `links.json` (§8.3); the inline form is for
  rendering, the link record is for the queryable graph.
- No other inline construct is part of the v0.1 dialect. Custom *inline* types are deferred
  (interface §8).

### 7.2 Canonical form

So that equal content always hashes equally (§9), writers **MUST** store the **canonical
form**, defined as the output of the normative normalizer (parse the inline fragment with the
dialect's grammar, then re-emit):

- emphasis delimiters are always `*` / `**` (never `_` / `__`); strikethrough is `~~`;
- code spans use the shortest backtick fence longer than any backtick run in the content,
  space-padded when the content starts or ends with a backtick;
- link destinations are wrapped in `<…>` only when they contain a space or parenthesis;
- soft/hard breaks collapse to a single space (inline content is one logical line);
- in plain text the characters `` \ ` * _ [ ] < ~ `` are backslash-escaped, `&` is escaped
  only where it would otherwise read as a character entity, and a leading `#`, `>`, `-`, `+`
  (or the `.`/`)` of a leading ordinal like `1.`) is escaped so a stored string can never
  re-parse as a different block;
- normalization is **idempotent**: the canonical form is a fixpoint of the normalizer.

### 7.3 Plain text

The **plain text** of a node is its content with markup stripped (parse, concatenate text).
Agents reading for content **SHOULD** use this projection — it is markup-free prose. The
spine `label` of a heading (§5.2) is its plain text.

### 7.4 Table content

A `table` node carries its **entire table** as one `content` string: the **canonical GFM
serialization**. *(Supersedes the structural `table` → `table-row` → `table-cell` model,
2026-06-04: per-cell nodes cost ~30× the content size in parts + spine entries — each entry
carries an id and hash — and cell-level addressing was unusable in practice since cells are
unlabeled in the spine. GFM already encodes everything the structural attrs did.)*

```json
{ "id": "t_lat", "type": "table",
  "content": "| Attempt | Delay |\n| :--- | ---: |\n| 1 | 1s |" }
```

- The **first row is the header**; **column alignment** lives in the separator row
  (`---` / `:---` / `:---:` / `---:`). There are no table attrs — GFM is self-describing.
- Cells contain canonical **inline** syntax (§7.1–§7.2); a literal `|` in a cell is
  backslash-escaped. Cells are single-line (no block content — unchanged from the
  structural model).
- **Canonical form**: `| a | b |` single-space padding, one separator row, escaped pipes,
  cells individually in inline canonical form. As with §7.2, writers MUST store the output
  of the normative normalizer (parse the GFM table, re-emit); normalization is idempotent.
- Inline `krg://` links inside cells follow §7.1 (mirrored to `links.json`).
- The plain text of a table is its cells' plain text in reading order, space-separated
  (pipes and the separator row are not text).

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
- Every inline `krg://` link (§7.1) **MUST** be reflected here; the reverse is not required
  (links may exist without an inline occurrence).

## 9. Content hashing & integrity

Each node's `hash` (stored in its spine entry) is the integrity check and the optimistic
concurrency token (it is *not* stored inside the node part, to avoid self-reference).

- **Algorithm:** SHA-256 over the **canonical serialization** of the node part, formatted as
  `"sha256:" + lowercase-hex`.
- **Canonical serialization:** UTF-8 JSON with object keys sorted lexicographically by Unicode
  code point and no insignificant whitespace. This is RFC 8785 / JCS **restricted to the
  Karanga value domain**, a deliberate restriction (§9.1) that makes canonicalization simple,
  reproducible, and dependency-free: within the domain, canonical form reduces to *sorted keys
  + compact + UTF-8*. The `x` extension bag is included in the hash.

### 9.1 The hashable value domain (no floating-point)

To keep canonicalization free of the only genuinely hard part of RFC 8785 — ECMAScript
floating-point number formatting — Karanga restricts the value domain of everything that is
hashed (i.e. all document data):

- **Numbers MUST be integers.** No floating-point values, no exponent notation. The node model
  needs none (`heading.level` is the only numeric core attribute). Implementations **MUST**
  reject a document containing a non-integer number in any node part, including `attrs` and
  `x`.
- **Object keys are ASCII** (`node_id`/`asset_id` charset plus the fixed field names), so
  Unicode-code-point and UTF-16 key ordering coincide — there is no non-BMP key-sorting edge
  case.
- Within this domain, a conforming canonicalizer is byte-identical to a full RFC 8785
  implementation, so third-party JCS libraries remain interoperable. The restriction lives only
  in *what values are permitted*, not in the algorithm.
- A writer performs an update as a **compare-and-swap**: it records a node's `hash` at read
  time and, before committing, confirms the on-disk node still hashes to that value; if not,
  the write is rejected as stale and the writer re-reads (§ optimistic concurrency, see
  REQUIREMENTS FR-23). No locking is defined by the format.

## 10. Extensibility & versioning

- **Spec version** (`manifest.krg`) is `MAJOR.MINOR`. A reader **MUST** reject a document
  whose `MAJOR` it does not implement. A reader **MUST** accept a higher `MINOR` of a `MAJOR`
  it implements, ignoring unknown additions but preserving them on round-trip.
- **Extension node types** are namespaced (`acme:callout`) and declared in the manifest `types`
  registry with a descriptor (§6.4); readers render them structurally from the descriptor. The
  vocabulary grows by **schema, not by spec version** — adding a custom type needs no `MAJOR`/
  `MINOR` bump. (Custom *inline* constructs are deferred, §7.1.) A non-base type that is *not*
  declared is invalid (§6.4); declared-but-unrecognized types are preserved and rendered
  structurally.
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

- ~~**A1.** Spine denormalization — *resolved:* accept the writer sync obligation; node part is
  source-of-truth on disagreement (§5.2).~~
- ~~**A2.** Block children in `blockquote`/`list-item` — *resolved (revised):* **supported in
  v0.1** via the generic container model — nested lists and multi-block quotes/items are
  first-class (§6.2). Supersedes the earlier "inline-only / flat lists" decision, once the
  type system became schema-driven.~~
- ~~**A3.** ID strategy — *resolved:* UUID `doc_id` + ULID `node_id`. Not content-addressed:
  content-addressed ids change on every edit, which would break stable refs/links (§3).~~
- ~~**A4.** `table` node type — *resolved (revised twice):* **included in the v0.1 base
  schema** as a single `table` node carrying its canonical GFM serialization (§7.4); the
  intermediate structural `table-row`/`table-cell` model was superseded 2026-06-04.
  Supersedes the earlier "defer to v0.2" decision.~~
- ~~**A7.** Extensible type system — *resolved:* the node-type vocabulary is schema-driven and
  open (§6.2–§6.4): a generic envelope + per-type descriptors + a document `types` registry,
  with structural rendering of declared-but-unrecognized types. New structured content is added
  by schema, not spec version.~~
- ~~**A5.** Canonical-JSON dependency — *resolved:* restrict the hashable value domain to
  integers + ASCII keys (§9.1), making a vendored minimal canonicalizer byte-identical to RFC
  8785 with no external dependency.~~
- **A6.** Grep-friendly content (deferred, two-way door): whether/when to add an optional,
  uncompressed full-text part — and a manifest flag to toggle it — so node **content** (not
  just titles) is discoverable by standard search on packed files. Omitted from v0.1's lean
  default; backward-compatible to add later (§2).
