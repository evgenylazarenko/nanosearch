use std::path::{Path, PathBuf};

/// Path to the source fixture repo (read-only — never write into this).
pub fn fixture_source() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample_repo")
}

/// Recursively copies `src` into `dst`, preserving directory structure.
pub fn copy_dir_recursive(src: &Path, dst: &Path) {
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

/// Creates an isolated copy of the fixture repo in a temp directory (not indexed).
pub fn isolated_fixture() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().expect("failed to create tempdir");
    let root = tmp.path().join("repo");
    copy_dir_recursive(&fixture_source(), &root);
    (tmp, root)
}

/// Creates an isolated copy of the fixture repo and indexes it.
pub fn indexed_fixture() -> (tempfile::TempDir, PathBuf) {
    let (tmp, root) = isolated_fixture();
    ns::indexer::run_full_index(&root, 1_048_576).expect("indexing should succeed");
    (tmp, root)
}
