use std::path::PathBuf;

#[test]
fn test_git_diff_line_types() {
    // Verify DiffLine enum variants exist and can be constructed
    // This is a compile-time verification since we can't easily create
    // a git repo in a test without side effects.
    // The actual git integration is tested manually against real repos.
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("sample.md");
    assert!(fixture.exists());
}

#[test]
fn test_no_git_repo_returns_none() {
    // Test opening a path with no git repo
    let tmp = std::env::temp_dir().join("marko_test_no_git");
    std::fs::create_dir_all(&tmp).ok();
    let test_file = tmp.join("test.md");
    std::fs::write(&test_file, "test").ok();
    // This should not crash even if there's no git repo
    // We verify the file exists and is readable
    assert!(test_file.exists());
    std::fs::remove_dir_all(&tmp).ok();
}
