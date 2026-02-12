use std::path::PathBuf;

/// Helper: path to the fixture repo
fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample_repo")
}

/// Helper: clean up .ns/ in the fixture repo
fn cleanup(root: &std::path::Path) {
    let ns_dir = root.join(".ns");
    if ns_dir.exists() {
        std::fs::remove_dir_all(&ns_dir).expect("failed to clean up .ns");
    }
}

#[test]
fn full_index_creates_ns_directory() {
    let root = fixture_root();
    cleanup(&root);

    let count = ns::indexer::run_full_index(&root, 1_048_576).expect("indexing should succeed");

    // Should index all source files + README + config.json
    assert!(count >= 6, "expected at least 6 files, got {}", count);

    // .ns/index/ and .ns/meta.json should exist
    assert!(root.join(".ns/index").is_dir(), ".ns/index/ should exist");
    assert!(root.join(".ns/meta.json").is_file(), ".ns/meta.json should exist");

    // Read and verify meta.json
    let meta = ns::indexer::writer::read_meta(&root).expect("should read meta.json");
    assert_eq!(meta.schema_version, 1);
    assert_eq!(meta.file_count, count);
    assert!(meta.index_size_bytes > 0);

    cleanup(&root);
}

#[test]
fn status_without_index_fails() {
    let root = fixture_root();
    cleanup(&root);

    let result = ns::indexer::writer::read_meta(&root);
    assert!(result.is_err(), "reading meta without index should fail");
}
