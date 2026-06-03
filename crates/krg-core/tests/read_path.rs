//! Read-path integration tests against the `retry-policy` conformance fixture
//! (the packed `.krg`).

use krg_core::document::Document;

fn fixture() -> String {
    format!(
        "{}/../../spec/examples/retry-policy.krg",
        env!("CARGO_MANIFEST_DIR")
    )
}

#[test]
fn opens_packed_krg() {
    let doc = Document::open(fixture()).expect("open .krg");
    assert_eq!(doc.manifest.title, "Retry Policy");
    assert_eq!(doc.manifest.krg, "0.1");
}

#[test]
fn outline_matches_expected() {
    let doc = Document::open(fixture()).unwrap();
    let expected = "\
Retry Policy   krg://9f1c2e4a-6b2d-4f8a-9c3e-1a2b3c4d5e6f/
- Overview  ⟨h_over⟩
- Methods  ⟨h_meth⟩
  - Results  ⟨h_res⟩
";
    assert_eq!(doc.outline(), expected);
}

#[test]
fn renders_paragraph_with_marks() {
    let doc = Document::open(fixture()).unwrap();
    let n = doc.node("p_intro").unwrap();
    assert_eq!(n.ty, "paragraph");
    assert_eq!(
        n.content,
        "Retries are capped at **three attempts**. See [the gateway guide](https://example.com/gateway) and the [Methods](krg:///h_meth) section."
    );
}

#[test]
fn renders_code_and_heading() {
    let doc = Document::open(fixture()).unwrap();
    assert!(doc.node("c_ex").unwrap().content.starts_with("```go\n"));
    assert_eq!(doc.node("h_res").unwrap().content, "## Results");
}

#[test]
fn rev_is_short_token() {
    let doc = Document::open(fixture()).unwrap();
    assert_eq!(doc.node("p_intro").unwrap().rev.0.len(), 12);
}

#[test]
fn missing_node_is_not_found() {
    let doc = Document::open(fixture()).unwrap();
    assert!(doc.node("nope").is_err());
}

#[test]
fn render_matches_expected_md() {
    let doc = Document::open(fixture()).unwrap();
    let expected = std::fs::read_to_string(format!(
        "{}/../../spec/examples/retry-policy.expected.md",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    assert_eq!(doc.render().unwrap(), expected);
}

#[test]
fn fixture_validates_clean() {
    // Recomputes every node's content hash via the Rust canonicalizer and
    // checks it against the spine hashes (written by the Python generator).
    // A clean result proves the two canonicalizations are byte-identical.
    let doc = Document::open(fixture()).unwrap();
    let issues = doc.validate().unwrap();
    assert!(issues.is_empty(), "validation issues: {issues:#?}");
}

#[test]
fn section_renders_subtree() {
    let doc = Document::open(fixture()).unwrap();
    let s = doc.section("h_res").unwrap();
    assert!(s.starts_with("## Results\n\n"), "got:\n{s}");
    assert!(s.contains("| Attempt | Delay |\n| :--- | ---: |\n| 1 | 1s |"));
    assert!(s.contains(":::acme:callout{variant=\"warn\"}\nNever retry on 4xx responses.\n:::"));
}
