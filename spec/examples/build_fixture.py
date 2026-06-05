#!/usr/bin/env python3
"""Generate the `retry-policy` conformance fixture for Karanga format v0.1.

Emits the exploded form under spec/examples/retry-policy/ and packs
spec/examples/retry-policy.krg. Node content hashes are computed over the
canonical serialization defined in format-v0.1 §9.1 (sorted keys, compact,
UTF-8, integer-only domain) — so the fixture is a valid `.krg`, not a sketch.

Run:  python3 spec/examples/build_fixture.py
"""
import base64
import hashlib
import json
import os
import zipfile

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "retry-policy")
KRG = os.path.join(HERE, "retry-policy.krg")
DOC_ID = "9f1c2e4a-6b2d-4f8a-9c3e-1a2b3c4d5e6f"
TS = "2026-06-02T00:00:00Z"
MIMETYPE = "application/vnd.karanga.document+zip"


def canon(obj):
    """Canonical JSON for hashing (format §9.1)."""
    return json.dumps(obj, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def node_hash(node):
    return "sha256:" + hashlib.sha256(canon(node).encode("utf-8")).hexdigest()


# --- nodes -------------------------------------------------------------------
# Inline content is a single string of *canonical* Karanga Markdown inline
# syntax (format §7): `**strong**`, `*em*`, `~~strike~~`, code spans, links
# (a `krg://` destination is an internal ref), markup characters escaped.
NODES = {
    "h_over": {"id": "h_over", "type": "heading", "attrs": {"level": 1},
               "content": "Overview"},
    "p_intro": {"id": "p_intro", "type": "paragraph",
                "content": "Retries are capped at **three attempts**. "
                           "See [the gateway guide](https://example.com/gateway) "
                           "and the [Methods](krg:///h_meth) section."},
    "q_note": {"id": "q_note", "type": "blockquote"},
    "p_note": {"id": "p_note", "type": "paragraph",
               "content": "Retries are only safe for idempotent requests."},

    "h_meth": {"id": "h_meth", "type": "heading", "attrs": {"level": 1},
               "content": "Methods"},
    "p_meth": {"id": "p_meth", "type": "paragraph",
               "content": "The backoff schedule is exponential:"},
    "l_bk": {"id": "l_bk", "type": "list", "attrs": {"ordered": True}},
    "li_1": {"id": "li_1", "type": "list-item"},
    "p_li1": {"id": "p_li1", "type": "paragraph",
              "content": "First retry after 1s."},
    "li_2": {"id": "li_2", "type": "list-item"},
    "p_li2": {"id": "p_li2", "type": "paragraph",
              "content": "Second retry after 2s, then:"},
    "l_sub": {"id": "l_sub", "type": "list", "attrs": {"ordered": False}},
    "li_2a": {"id": "li_2a", "type": "list-item"},
    "p_li2a": {"id": "p_li2a", "type": "paragraph",
               "content": "with full jitter applied."},
    "c_ex": {"id": "c_ex", "type": "code", "attrs": {"language": "go"},
             "content": "func backoff(n int) time.Duration {\n\treturn (1 << n) * time.Second\n}"},

    "h_res": {"id": "h_res", "type": "heading", "attrs": {"level": 2},
              "content": "Results"},
    # A table is a single node: content = the canonical GFM serialization
    # (format §7.4; header = first row, alignment in the separator row).
    "t_lat": {"id": "t_lat", "type": "table",
              "content": "| Attempt | Delay |\n| :--- | ---: |\n| 1 | 1s |"},
    "m_graph": {"id": "m_graph", "type": "media",
                "attrs": {"media_kind": "image", "asset": "latency",
                          "alt": "p95 latency by attempt",
                          "caption": "Latency rises with each attempt"}},
    "d_div": {"id": "d_div", "type": "divider"},
    "cl_warn": {"id": "cl_warn", "type": "acme:callout", "attrs": {"variant": "warn"}},
    "p_warn": {"id": "p_warn", "type": "paragraph",
               "content": "Never retry on 4xx responses."},
}

# --- spine skeleton (reading order; nesting = containment) ------------------
SKELETON = [
    ("h_over", [("p_intro", []), ("q_note", [("p_note", [])])]),
    ("h_meth", [
        ("p_meth", []),
        ("l_bk", [
            ("li_1", [("p_li1", [])]),
            ("li_2", [("p_li2", []), ("l_sub", [("li_2a", [("p_li2a", [])])])]),
        ]),
        ("c_ex", []),
        ("h_res", [
            ("t_lat", []),
            ("m_graph", []),
            ("d_div", []),
            ("cl_warn", [("p_warn", [])]),
        ]),
    ]),
]

LINKS = {"links": [{"from": "p_intro", "to": "krg:///h_meth", "type": "ref"}]}

MANIFEST = {
    "krg": "0.1",
    "doc_id": DOC_ID,
    "title": "Retry Policy",
    "description": "How the gateway retries upstream failures.",
    "created": TS,
    "modified": TS,
    "media_mode": "embedded",
    "authors": [{"name": "Cameron G. Gould"}],
    "types": {
        "acme:callout": {
            "content": "empty",
            "children": "block",
            "attrs": {"variant": "string"},
            "render": {"hint": "callout"},
        }
    },
}

# 1x1 transparent PNG
PNG = base64.b64decode(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg=="
)


def label_of(node):
    # Spine label = the heading's plaintext. The fixture's headings carry no
    # inline markup, so the stored string is already plain.
    if node["type"] == "heading":
        return node["content"]
    return None


def build_entry(item):
    nid, kids = item
    node = NODES[nid]
    entry = {"id": nid, "type": node["type"], "hash": node_hash(node)}
    lbl = label_of(node)
    if lbl is not None:
        entry["label"] = lbl
    if kids:
        entry["children"] = [build_entry(k) for k in kids]
    return entry


def write_json(path, obj):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        json.dump(obj, f, ensure_ascii=False, indent=2)
        f.write("\n")


def main():
    # exploded form — clear nodes/ first so removed nodes don't survive as
    # orphan parts (validate would flag them)
    import shutil
    shutil.rmtree(os.path.join(OUT, "nodes"), ignore_errors=True)
    os.makedirs(OUT, exist_ok=True)
    with open(os.path.join(OUT, "mimetype"), "w", encoding="utf-8") as f:
        f.write(MIMETYPE)  # no trailing newline
    write_json(os.path.join(OUT, "manifest.json"), MANIFEST)
    for nid, node in NODES.items():
        write_json(os.path.join(OUT, "nodes", nid + ".json"), node)
    spine = {"root": [build_entry(i) for i in SKELETON]}
    write_json(os.path.join(OUT, "spine.json"), spine)
    write_json(os.path.join(OUT, "links.json"), LINKS)
    os.makedirs(os.path.join(OUT, "media"), exist_ok=True)
    with open(os.path.join(OUT, "media", "latency.png"), "wb") as f:
        f.write(PNG)

    # packed .krg — mimetype first and STOREd, everything else DEFLATEd
    with zipfile.ZipFile(KRG, "w") as z:
        z.writestr(zipfile.ZipInfo("mimetype"), MIMETYPE, compress_type=zipfile.ZIP_STORED)
        z.write(os.path.join(OUT, "manifest.json"), "manifest.json", compress_type=zipfile.ZIP_STORED)
        z.write(os.path.join(OUT, "spine.json"), "spine.json", compress_type=zipfile.ZIP_DEFLATED)
        z.write(os.path.join(OUT, "links.json"), "links.json", compress_type=zipfile.ZIP_DEFLATED)
        for nid in NODES:
            z.write(os.path.join(OUT, "nodes", nid + ".json"), "nodes/" + nid + ".json",
                    compress_type=zipfile.ZIP_DEFLATED)
        z.write(os.path.join(OUT, "media", "latency.png"), "media/latency.png",
                compress_type=zipfile.ZIP_DEFLATED)

    print("wrote", OUT, "and", KRG)
    print("nodes:", len(NODES))


if __name__ == "__main__":
    main()
