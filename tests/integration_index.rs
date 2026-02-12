mod common;

#[test]
fn full_index_creates_ns_directory() {
    let (_tmp, root) = common::isolated_fixture();

    let count = ns::indexer::run_full_index(&root, 1_048_576).expect("indexing should succeed");

    assert!(count >= 6, "expected at least 6 files, got {}", count);

    assert!(root.join(".ns/index").is_dir(), ".ns/index/ should exist");
    assert!(
        root.join(".ns/meta.json").is_file(),
        ".ns/meta.json should exist"
    );

    let meta = ns::indexer::writer::read_meta(&root).expect("should read meta.json");
    assert_eq!(meta.schema_version, 1);
    assert_eq!(meta.file_count, count);
    assert!(meta.index_size_bytes > 0);
    assert!(meta.indexed_at.contains('T'), "indexed_at should be ISO 8601");
}

#[test]
fn reindex_is_idempotent() {
    let (_tmp, root) = common::isolated_fixture();

    let count1 = ns::indexer::run_full_index(&root, 1_048_576).expect("first index should succeed");
    let count2 =
        ns::indexer::run_full_index(&root, 1_048_576).expect("second index should succeed");

    assert_eq!(count1, count2, "re-index should produce same file count");
}

#[test]
fn status_without_index_fails() {
    let (_tmp, root) = common::isolated_fixture();

    let result = ns::indexer::writer::read_meta(&root);
    assert!(result.is_err(), "reading meta without index should fail");
}
