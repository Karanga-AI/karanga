# Karanga — Initial Requirements

**Status:** Draft / pre-implementation
**Last updated:** 2026-06-01

This document captures the initial requirements for Karanga. It is the working
specification of *what the system must do and why*, prior to detailed design of the
`.krg` format and the reference implementation. It will evolve; requirement IDs are
stable so they can be referenced from design docs, issues, and tests.

---

## 1. Purpose & scope

Karanga is a document format and editing system for **hybrid human/agent collaboration**.
A single document must simultaneously serve:

- a **human**, who creates, reads, and edits it as a traditional flowing document; and
- an **agent**, which searches, navigates, and retrieves its contents at the granularity
  of individual nodes, minimizing context consumption.

The deliverables in scope are: the `.krg` file format and its specification; a core
library implementing it; a CLI reader/writer; an MCP server exposing query/edit verbs to
agents; format converters; and a cross-platform desktop editor.

## 2. Terminology

| Term | Meaning |
|---|---|
| **Document** | A complete `.krg` package — the unit a human creates and shares. |
| **Node** | An atomic, typed, individually-addressable unit of content (heading, paragraph, quote, code, list item, media, divider, …). |
| **Spine** | The ordered, hierarchical tree of node references that reconstructs the document. |
| **Manifest** | Document-level metadata: title, description, spec version. |
| **Link** | A typed, directed relationship between nodes, identified by global node-id. |
| **Packed / exploded** | A `.krg` as a single zip (at rest) vs. a working directory of its parts (while editing). |
| **Consumer** | The human reader/author. |
| **Agent** | An automated client operating via the query/edit verbs. |

## 3. Governing constraints

These are **hard constraints**. No requirement or design may violate one.

- **C1 — Atomic.** Data is decoupled and linked; retrievable per-node so an agent need not read a whole document.
- **C2 — Reconstructable.** The same data must be creatable, editable, and reconstructable as a traditional document view.
- **C3 — Tiered query.** Content is discoverable document-first, then document-index, then node.
- **C4 — Multimedia.** Media is supported; the client is responsible for rendering it.
- **C5 — No leakage.** The consumer never deals with the underlying structure; the agent may. (Subsumes the earlier "no SQL / no query language exposed" rule.)
- **C6 — Lightweight, portable, encapsulated.**

> Two earlier constraints have been **retired as resolved, not relaxed**: *justified substrate* (the `.krg` package format was chosen and justified) and *no SQL exposed* (there is no database at all — the package structure is the index — so the rule is moot and is folded into C5).

## 4. Functional requirements

### 4.1 The `.krg` format

- **FR-1** A `.krg` document SHALL be a package (zip-based) containing a manifest, a spine, a set of node parts, a link graph, and embedded media. *(C1, C6)*
- **FR-2** Each node SHALL have a stable, globally unique identifier so it can be referenced across documents. *(C1)*
- **FR-3** Each node SHALL declare a type from a defined, extensible vocabulary of segment types (e.g. heading, paragraph, blockquote, code, list, list-item, media, divider). *(C1, C3)*
- **FR-4** The spine SHALL define sibling order and hierarchical nesting sufficient to losslessly reconstruct the document's reading order and section structure. *(C2)*
- **FR-5** A reader SHALL be able to extract any single node without parsing other node parts. *(C1)*
- **FR-6** The manifest SHALL record the document title, an optional short description, and the format spec version the file conforms to. *(C3)*
- **FR-7** Media SHALL be storable either as embedded parts (fully self-contained document) or as external references; the chosen mode SHALL be recorded so a renderer can resolve it. *(C4, C6)*
- **FR-8** Links SHALL be first-class, typed, and directed, recorded by global node-id, and SHALL support both intra-document and inter-document targets. *(C1)*

### 4.2 Query (the three tiers)

- **FR-9** **Tier 1 — discovery.** Given a query, the system SHALL return candidate documents using manifest-level data (title/description) only, without reading node bodies. *(C3)*
- **FR-10** **Tier 2 — index.** Given a document, the system SHALL return its outline (the spine: headings/sections and their node-ids) without returning body content. *(C3)*
- **FR-11** **Tier 3 — node.** Given a node-id, the system SHALL return exactly that node's content. *(C1, C3)*
- **FR-12** The system SHALL support querying/filtering nodes by segment type (e.g. "all blockquotes in this document"). *(C3)*
- **FR-13** The system SHALL support traversing links from a given node (outgoing and incoming). *(C1)*
- **FR-14** No query interface SHALL require or expose a database or query language (SQL or otherwise) to the consumer or the agent; access is via intent-shaped verbs only. *(C5)*

### 4.3 Reconstruction & rendering

- **FR-15** The system SHALL reconstruct a complete document from its nodes and spine and render it to at least one human-faithful presentation (e.g. Markdown/HTML), preserving authored order and structure. *(C2)*
- **FR-16** Rendering SHALL be deterministic: the same document yields the same output. *(C2)*

### 4.4 Authoring & editing

- **FR-17** The system SHALL support creating a document, and inserting, updating, moving, and deleting nodes. *(C2)*
- **FR-18** A consumer editing through the client SHALL interact only with the rendered document; node structure, ids, and the package layout SHALL NOT be surfaced to them. *(C5)*
- **FR-19** Edits SHALL operate on the exploded form; the system SHALL repack to a valid `.krg` on save/close. *(C6)*

### 4.5 Concurrent editing

> The format is **passive data** — like `.docx`, a `.krg` imposes no concurrency control of its own.
> Coordination is a behavior of the writer library, not a property of the file, and requires no server.

- **FR-20** The `.krg` format SHALL NOT define or require any locking, leasing, or session mechanism; concurrency coordination SHALL be a concern of the runtime/writer, never of the file. *(C6)*
- **FR-21** A human (via the editor) and one or more agents (via MCP/CLI) SHALL be able to edit the same document concurrently **without a central coordinating service**. *(C2, C6)*
- **FR-22** Concurrent edits to **different** nodes SHALL never conflict.
- **FR-23** Node writes SHALL use **optimistic concurrency**: each node part carries a content hash, and a write SHALL be applied via atomic compare-and-swap (apply only if the on-disk node is unchanged since it was read). A stale write SHALL return a structured "changed underneath — re-read" result rather than clobbering; the only resolution required for a same-node clash is re-read and retry. No locks, leases, heartbeats, or daemon SHALL be required. *(C6)*
- **FR-24** The sole permitted lock SHALL be a brief advisory guard during **repack** (exploded → `.krg`) to prevent simultaneous-write corruption of the archive; it SHALL require no persistent service. *(C6)*

### 4.6 Agent interface (MCP)

- **FR-25** Agent capabilities SHALL be exposed as a small, fixed set of intent-shaped verbs (discovery, outline, get-node, query-by-type, traverse-links, and the editing operations), not as a query language. *(C5)*
- **FR-26** Verb descriptions SHALL guide the agent toward the cheapest-first tiered access pattern.
- **FR-27** The agent interface SHALL NOT require the agent to understand the `.krg` package layout to use it. *(C5)*

### 4.7 CLI (reader/writer)

- **FR-28** A CLI SHALL provide commands for the query tiers and rendering (at minimum: search, outline, get, render, links). *(C3)*
- **FR-29** The CLI SHALL operate on `.krg` files directly without a running service. *(C6)*

### 4.8 Converters

- **FR-30** The system SHALL provide lossless-where-possible conversion between `.krg` and at least Markdown, with `.docx` and other formats as goals, to avoid format lock-in. *(NFR-6)*

## 5. Non-functional requirements

- **NFR-1 — Portability.** A `.krg` document SHALL be a single self-contained file usable across platforms with no external dependencies. *(C6)*
- **NFR-2 — No server required.** Reading, querying, editing, and converting a `.krg` SHALL be possible with the CLI/library alone, offline, with no daemon or database. *(C6)*
- **NFR-3 — Cross-platform.** The editor SHALL run on Windows, macOS, and Linux.
- **NFR-4 — Lightweight footprint.** The core library and CLI SHALL ship as small native binaries.
- **NFR-5 — Single source of truth.** There SHALL be no derived index that can drift from the document; the package structure is authoritative. Any optional acceleration index SHALL be rebuildable and hidden. *(C6)*
- **NFR-6 — Standardizability.** The format SHALL be specified independently of any implementation, with a conformance test suite that implementations validate against.
- **NFR-7 — Versioned format.** Every document SHALL record its spec version; the spec SHALL be independently versioned (semver). Readers SHALL declare which spec versions they support.

## 6. The `.krg` format (initial sketch)

> Detailed schemas live in the spec; this is the orienting shape.

```
mydoc.krg                       (zip container)
├── manifest.json               { title, description, spec_version, media_mode, ... }
├── spine.json                  ordered tree of { node_id, children[] }
├── nodes/
│   └── <node_id>.json          { id, type, content, attrs, ... }
├── links.json                  [ { from, to, type } ]
└── media/
    └── <asset>                 embedded blobs (when media_mode = embedded)
```

## 7. Non-goals (initial)

- Pixel-perfect / fixed-layout fidelity (pagination, precise typography). Karanga targets *semantic* rich documents, not a layout engine. *(revisit if required)*
- Real-time multi-device offline-merge (CRDT) collaboration. Optimistic node-level concurrency assumes writers share a filesystem; conflict-free offline / multi-device merge (and same-node character-level merge) is explicitly out of scope for v1.
- Acting as a general knowledge base / memory system.

## 8. Open questions

- **Sections:** derive the Tier-2 index from heading nodes vs. model sections as explicit container nodes in the spine.
- **Media:** default to embedded vs. referenced; thresholds for each.
- **Optional acceleration index:** if/when corpus scale warrants a hidden sidecar (e.g. full-text), and where it lives.
- **Node content encoding:** inline formatting model within a text node (marks/spans) and how faithfully it round-trips.
- **Repository layout:** currently a single `karanga` repo; the spec + conformance suite may later split out to support third-party implementations.
