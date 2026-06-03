# Conformance fixtures

Worked examples that double as conformance fixtures for the Karanga format
(`../format-v0.1.md`) and operation interface (`../interface-v0.1.md`). The JSON
Schemas live in `../schemas/`.

## `retry-policy`

A single document exercising the full v0.1 surface:

| Feature | Exercised by |
|---|---|
| Sections (heading-as-container) + nesting | `h_over`, `h_meth`, `h_res` (an `h2` nested inside an `h1` section) |
| Inline marks | `p_intro`: `strong`, a `link` (external), and a `ref` (internal) |
| Internal link → `links.json` | `p_intro` `ref1` → `links.json` entry → `krg:///h_meth` |
| Blockquote as container | `q_note` → `p_note` |
| Code (raw content) | `c_ex` (`language: go`, with a tab) |
| Ordered list + **nested** list | `l_bk` → `li_2` → `l_sub` (reversed A2) |
| Table (base-schema) | `t_lat` → `table-row` → `table-cell` (header row + `align`) |
| Embedded media | `m_graph` → `media/latency.png`, `media_mode: embedded` |
| Divider | `d_div` |
| **Custom type via the `types` registry** | `acme:callout` (`cl_warn`), declared in `manifest.types` |

### Files

- `retry-policy/` — the **exploded** form (the canonical fixture content): `mimetype`,
  `manifest.json`, `spine.json`, `nodes/*.json`, `links.json`, `media/`.
- `retry-policy.krg` — the **packed** form (a ZIP; `mimetype` first + `STORE`d, `manifest.json`
  `STORE`d per the lean compression policy, the rest `DEFLATE`d).
- `retry-policy.expected.md` — the deterministic render of the spine (Karanga Markdown). The
  document title is held in the manifest and is not prepended by `render` in this fixture.
- `retry-policy.outline.txt` — the expected `get_outline` projection (headings only, nested).

### Conformance assertions

A conforming implementation should satisfy:

1. **Container**: `retry-policy.krg` is a valid ZIP; `mimetype` is the first entry and `STORE`d;
   `manifest.json` is `STORE`d (grep-able titles).
2. **Schema**: each part validates against its schema in `../schemas/`.
3. **Integrity**: every `spine.json` `hash` equals `"sha256:" + sha256(canonical(node))`, where
   `canonical` is the §9.1 canonical JSON (sorted keys, compact, UTF-8, integer-only domain).
4. **Structure**: the spine is acyclic and in bijection with `nodes/`; its `type`/`label`
   projections match the node parts.
5. **Custom types**: `acme:callout` is declared in `manifest.types`; a reader renders it
   structurally (as a `:::acme:callout{…}` directive) from its descriptor.
6. **Render**: `render(retry-policy)` equals `retry-policy.expected.md`.
7. **Outline**: `get_outline(retry-policy)` equals `retry-policy.outline.txt`.

### Regenerating

The exploded form and the `.krg` are produced by `build_fixture.py` (the hashes are computed,
not hand-written, so the fixture is genuinely valid):

```
python3 spec/examples/build_fixture.py
```

The expected-output files (`*.expected.md`, `*.outline.txt`) are **authored by hand** — they are
the targets implementations are checked against, not generated. Eventually the Rust conformance
tooling will replace the Python generator and assert the outputs.
