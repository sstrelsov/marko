use std::fs;
use std::process::Command;
use tempfile::TempDir;

use marko::pandoc;

/// Helper: skip test if pandoc is not installed.
fn require_pandoc() -> bool {
    pandoc::is_available()
}

/// Helper: build the marko binary path for integration tests.
fn marko_bin() -> std::path::PathBuf {
    // Use cargo to find the binary
    let mut path = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    path.push("marko");
    path
}

#[test]
fn export_creates_docx_file() {
    if !require_pandoc() {
        return;
    }
    let dir = TempDir::new().unwrap();
    let md_path = dir.path().join("test.md");
    fs::write(&md_path, "# Hello\n\nWorld").unwrap();

    let output = Command::new(marko_bin())
        .args(["export", md_path.to_str().unwrap()])
        .output()
        .expect("failed to run marko export");

    assert!(output.status.success(), "marko export should succeed: {:?}", String::from_utf8_lossy(&output.stderr));

    let docx_path = dir.path().join("test.docx");
    assert!(docx_path.exists(), "test.docx should be created");
    assert!(fs::metadata(&docx_path).unwrap().len() > 0);
}

#[test]
fn export_respects_output_flag() {
    if !require_pandoc() {
        return;
    }
    let dir = TempDir::new().unwrap();
    let md_path = dir.path().join("input.md");
    let docx_path = dir.path().join("custom_output.docx");
    fs::write(&md_path, "# Test\n\nContent").unwrap();

    let output = Command::new(marko_bin())
        .args([
            "export",
            md_path.to_str().unwrap(),
            "-o",
            docx_path.to_str().unwrap(),
        ])
        .output()
        .expect("failed to run marko export");

    assert!(output.status.success(), "marko export -o should succeed: {:?}", String::from_utf8_lossy(&output.stderr));
    assert!(docx_path.exists(), "custom_output.docx should be created");
}

#[test]
fn export_with_reference_doc() {
    if !require_pandoc() {
        return;
    }
    let dir = TempDir::new().unwrap();
    let md_path = dir.path().join("input.md");
    let ref_path = dir.path().join("reference.docx");
    let out_path = dir.path().join("styled.docx");
    fs::write(&md_path, "# Styled\n\nContent").unwrap();

    // Create a reference doc first
    pandoc::md_to_docx(&md_path, &ref_path, None).unwrap();

    let output = Command::new(marko_bin())
        .args([
            "export",
            md_path.to_str().unwrap(),
            "-o",
            out_path.to_str().unwrap(),
            "--reference-doc",
            ref_path.to_str().unwrap(),
        ])
        .output()
        .expect("failed to run marko export");

    assert!(output.status.success(), "marko export --reference-doc should succeed");
    assert!(out_path.exists());
}

#[test]
fn export_fails_for_nonexistent_file() {
    let output = Command::new(marko_bin())
        .args(["export", "/tmp/does_not_exist_12345.md"])
        .output()
        .expect("failed to run marko export");

    assert!(!output.status.success(), "should fail for nonexistent file");
}

#[test]
fn docx_state_is_none_for_regular_md_via_lib() {
    use marko::app::App;
    use std::io::Write;
    use tempfile::NamedTempFile;

    let mut tmp = NamedTempFile::new().unwrap();
    tmp.write_all(b"hello world").unwrap();
    tmp.flush().unwrap();
    let app = App::new(tmp.path().to_path_buf());
    assert!(app.docx_state.is_none(), "docx_state should be None for .md files");
}
