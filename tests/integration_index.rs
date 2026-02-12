use std::path::{Path, PathBuf};

/// Path to the source fixture repo (read-only — never write into this).
fn fixture_source() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample_repo")
}

/// Recursively copies `src` into `dst`, preserving directory structure.
fn copy_dir_recursive(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).expect("failed to create dst dir");
    for entry in std::fs::read_dir(src).expect("failed to read src dir") {
        let entry = entry.expect("failed to read entry");
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path);
        } else {
            std::fs::copy(&src_path, &dst_path).expect("failed to copy file");
        }
    }
}

/// Creates an isolated copy of the fixture repo in a temp directory.
/// Returns the TempDir (holds the lifetime) and the path to the copied repo root.
fn isolated_fixture() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().expect("failed to create tempdir");
    let root = tmp.path().join("repo");
    copy_dir_recursive(&fixture_source(), &root);
    (tmp, root)
}

#[test]
fn full_index_creates_ns_directory() {
    let (_tmp, root) = isolated_fixture();

    let count = ns::indexer::run_full_index(&root, 1_048_576).expect("indexing should succeed");

    // Should index all source files + README + config.json
    assert!(count >= 6, "expected at least 6 files, got {}", count);

    // .ns/index/ and .ns/meta.json should exist
    assert!(root.join(".ns/index").is_dir(), ".ns/index/ should exist");
    assert!(
        root.join(".ns/meta.json").is_file(),
        ".ns/meta.json should exist"
    );

    // Read and verify meta.json
    let meta = ns::indexer::writer::read_meta(&root).expect("should read meta.json");
    assert_eq!(meta.schema_version, 1);
    assert_eq!(meta.file_count, count);
    assert!(meta.index_size_bytes > 0);
    // Timestamp should be a valid ISO 8601 string, not "unknown"
    assert!(meta.indexed_at.contains('T'), "indexed_at should be ISO 8601");
}

#[test]
fn reindex_is_idempotent() {
    let (_tmp, root) = isolated_fixture();

    // Index twice — second run should succeed (not error on existing .ns/index/)
    let count1 = ns::indexer::run_full_index(&root, 1_048_576).expect("first index should succeed");
    let count2 =
        ns::indexer::run_full_index(&root, 1_048_576).expect("second index should succeed");

    assert_eq!(count1, count2, "re-index should produce same file count");
}

#[test]
fn status_without_index_fails() {
    let (_tmp, root) = isolated_fixture();

    let result = ns::indexer::writer::read_meta(&root);
    assert!(result.is_err(), "reading meta without index should fail");
}
