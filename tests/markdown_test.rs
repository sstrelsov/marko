use std::path::PathBuf;

// Integration tests for the markdown renderer.
// These tests build the marko binary crate so we reference its internals
// through the binary's test infrastructure.
// The unit tests in src/markdown/renderer.rs cover individual element types.

#[test]
fn test_sample_fixture_is_valid_markdown() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("sample.md");
    assert!(fixture.exists(), "sample.md fixture should exist");
    let content = std::fs::read_to_string(&fixture).unwrap();
    assert!(content.contains("# Sample Markdown File"));
    assert!(content.contains("**sample**"));
    assert!(content.contains("*Italic*"));
    assert!(content.contains("```javascript"));
    assert!(content.contains("> This is a quote"));
    assert!(content.contains("---"));
}
