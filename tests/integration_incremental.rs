mod common;

use ns::searcher::query::SearchOptions;
use std::fs;
use std::thread;
use std::time::Duration;

/// Helper to build default SearchOptions.
fn opts(max: usize) -> SearchOptions {
    SearchOptions {
        max_results: max,
        ..Default::default()
    }
}

// ── Mtime-based tests (no git repo) ─────────────────────────────────────────

#[test]
fn incremental_no_changes_is_noop() {
    let (_tmp, root) = common::indexed_fixture();

    // Small delay so mtime comparison works
    thread::sleep(Duration::from_millis(50));

    let stats = ns::indexer::run_incremental_index(&root, 1_048_576)
        .expect("incremental should succeed");

    assert_eq!(stats.added, 0);
    assert_eq!(stats.modified, 0);
    assert_eq!(stats.deleted, 0);
}

#[test]
fn incremental_without_index_fails() {
    let (_tmp, root) = common::isolated_fixture();

    let result = ns::indexer::run_incremental_index(&root, 1_048_576);
    assert!(result.is_err(), "incremental without existing index should fail");
}

#[test]
fn incremental_detects_added_file_mtime() {
    let (_tmp, root) = common::indexed_fixture();

    // Wait a moment so the new file has a later mtime than indexed_at
    thread::sleep(Duration::from_secs(1));

    // Add a new Rust file
    let new_file = root.join("src").join("new_module.rs");
    fs::write(
        &new_file,
        "pub struct NewThing {\n    pub value: i32,\n}\n\npub fn create_new_thing() -> NewThing {\n    NewThing { value: 42 }\n}\n",
    )
    .expect("should write new file");

    let stats = ns::indexer::run_incremental_index(&root, 1_048_576)
        .expect("incremental should succeed");

    assert!(stats.added >= 1, "should detect at least 1 added file, got {}", stats.added);

    // Verify the new content is searchable
    let (results, _) = ns::searcher::query::execute_search(&root, "NewThing", &opts(10))
        .expect("search should work");

    assert!(!results.is_empty(), "should find results for 'NewThing'");
    assert!(
        results.iter().any(|r| r.path.contains("new_module.rs")),
        "new_module.rs should be in search results"
    );
}

#[test]
fn incremental_detects_modified_file_mtime() {
    let (_tmp, root) = common::indexed_fixture();

    // Wait so mtime changes
    thread::sleep(Duration::from_secs(1));

    // Modify an existing file — add a new struct
    let file_path = root.join("src").join("event_store.rs");
    let mut content = fs::read_to_string(&file_path).expect("should read file");
    content.push_str("\npub struct IncrementalTestMarker;\n");
    fs::write(&file_path, &content).expect("should write modified file");

    let stats = ns::indexer::run_incremental_index(&root, 1_048_576)
        .expect("incremental should succeed");

    assert!(
        stats.modified >= 1,
        "should detect at least 1 modified file, got {}",
        stats.modified
    );

    // Verify the new content is searchable
    let (results, _) =
        ns::searcher::query::execute_search(&root, "IncrementalTestMarker", &opts(10))
            .expect("search should work");

    assert!(
        !results.is_empty(),
        "should find results for 'IncrementalTestMarker'"
    );
}

#[test]
fn incremental_detects_deleted_file_mtime() {
    let (_tmp, root) = common::indexed_fixture();

    // Verify utils.js is searchable before deletion
    let (results_before, _) =
        ns::searcher::query::execute_search(&root, "debounce", &opts(10))
            .expect("search should work");
    assert!(
        results_before.iter().any(|r| r.path.contains("utils.js")),
        "utils.js should be in search results before deletion"
    );

    // Delete a file
    fs::remove_file(root.join("src").join("utils.js")).expect("should delete file");

    let stats = ns::indexer::run_incremental_index(&root, 1_048_576)
        .expect("incremental should succeed");

    assert!(
        stats.deleted >= 1,
        "should detect at least 1 deleted file, got {}",
        stats.deleted
    );

    // Verify the deleted file is no longer in search results
    let (results_after, _) =
        ns::searcher::query::execute_search(&root, "debounce", &opts(10))
            .expect("search should work");

    let still_has_utils = results_after.iter().any(|r| r.path.contains("utils.js"));
    assert!(
        !still_has_utils,
        "utils.js should not be in search results after deletion"
    );
}

#[test]
fn incremental_updates_meta_json() {
    let (_tmp, root) = common::indexed_fixture();

    let meta_before = ns::indexer::writer::read_meta(&root).expect("should read meta");

    thread::sleep(Duration::from_secs(1));

    // Add a file to trigger an actual change
    let new_file = root.join("src").join("meta_test.rs");
    fs::write(&new_file, "pub fn meta_test_fn() {}\n").expect("should write file");

    ns::indexer::run_incremental_index(&root, 1_048_576)
        .expect("incremental should succeed");

    let meta_after = ns::indexer::writer::read_meta(&root).expect("should read updated meta");

    assert!(
        meta_after.file_count > meta_before.file_count,
        "file count should increase after adding a file"
    );
    assert_ne!(
        meta_before.indexed_at, meta_after.indexed_at,
        "indexed_at should change after incremental update"
    );
}

// ── Git-based tests ─────────────────────────────────────────────────────────

/// Creates an isolated fixture with a git repo initialized and initial commit made.
fn git_indexed_fixture() -> (tempfile::TempDir, std::path::PathBuf) {
    let (tmp, root) = common::isolated_fixture();

    // Init git repo and commit all files
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(&root)
        .output()
        .expect("git init should succeed");

    std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(&root)
        .output()
        .expect("git add should succeed");

    std::process::Command::new("git")
        .args([
            "-c", "user.name=Test",
            "-c", "user.email=test@test.com",
            "commit", "-m", "initial commit",
        ])
        .current_dir(&root)
        .output()
        .expect("git commit should succeed");

    // Now index — meta.json will capture the git commit hash
    ns::indexer::run_full_index(&root, 1_048_576).expect("indexing should succeed");

    (tmp, root)
}

#[test]
fn incremental_git_detects_added_file() {
    let (_tmp, root) = git_indexed_fixture();

    // Add a new file and commit
    let new_file = root.join("src").join("git_added.rs");
    fs::write(
        &new_file,
        "pub struct GitAddedStruct;\n",
    )
    .expect("should write new file");

    std::process::Command::new("git")
        .args(["add", "src/git_added.rs"])
        .current_dir(&root)
        .output()
        .expect("git add should succeed");

    std::process::Command::new("git")
        .args([
            "-c", "user.name=Test",
            "-c", "user.email=test@test.com",
            "commit", "-m", "add git_added.rs",
        ])
        .current_dir(&root)
        .output()
        .expect("git commit should succeed");

    let stats = ns::indexer::run_incremental_index(&root, 1_048_576)
        .expect("incremental should succeed");

    assert!(stats.added >= 1, "should detect added file via git, got {} added", stats.added);

    // Verify searchable
    let (results, _) = ns::searcher::query::execute_search(&root, "GitAddedStruct", &opts(10))
        .expect("search should work");

    assert!(
        !results.is_empty(),
        "should find results for 'GitAddedStruct' after incremental"
    );
}

#[test]
fn incremental_git_detects_modified_file() {
    let (_tmp, root) = git_indexed_fixture();

    // Modify a file and commit
    let file_path = root.join("src").join("event_store.rs");
    let mut content = fs::read_to_string(&file_path).expect("should read");
    content.push_str("\npub struct GitModifiedMarker;\n");
    fs::write(&file_path, &content).expect("should write");

    std::process::Command::new("git")
        .args(["add", "src/event_store.rs"])
        .current_dir(&root)
        .output()
        .expect("git add should succeed");

    std::process::Command::new("git")
        .args([
            "-c", "user.name=Test",
            "-c", "user.email=test@test.com",
            "commit", "-m", "modify event_store.rs",
        ])
        .current_dir(&root)
        .output()
        .expect("git commit should succeed");

    let stats = ns::indexer::run_incremental_index(&root, 1_048_576)
        .expect("incremental should succeed");

    assert!(
        stats.modified >= 1,
        "should detect modified file via git, got {} modified",
        stats.modified
    );

    // Verify new content searchable
    let (results, _) =
        ns::searcher::query::execute_search(&root, "GitModifiedMarker", &opts(10))
            .expect("search should work");

    assert!(
        !results.is_empty(),
        "should find 'GitModifiedMarker' after incremental"
    );
}

#[test]
fn incremental_git_detects_deleted_file() {
    let (_tmp, root) = git_indexed_fixture();

    // Delete a file and commit
    fs::remove_file(root.join("src").join("utils.js")).expect("should delete");

    std::process::Command::new("git")
        .args(["add", "src/utils.js"])
        .current_dir(&root)
        .output()
        .expect("git add should succeed");

    std::process::Command::new("git")
        .args([
            "-c", "user.name=Test",
            "-c", "user.email=test@test.com",
            "commit", "-m", "delete utils.js",
        ])
        .current_dir(&root)
        .output()
        .expect("git commit should succeed");

    let stats = ns::indexer::run_incremental_index(&root, 1_048_576)
        .expect("incremental should succeed");

    assert!(
        stats.deleted >= 1,
        "should detect deleted file via git, got {} deleted",
        stats.deleted
    );

    // Verify deleted file is gone from results
    let (results, _) = ns::searcher::query::execute_search(&root, "debounce", &opts(10))
        .expect("search should work");

    let has_utils = results.iter().any(|r| r.path.contains("utils.js"));
    assert!(!has_utils, "utils.js should not be in results after git deletion");
}

#[test]
fn incremental_git_no_changes() {
    let (_tmp, root) = git_indexed_fixture();

    let stats = ns::indexer::run_incremental_index(&root, 1_048_576)
        .expect("incremental should succeed");

    assert_eq!(stats.added, 0, "no files should be added");
    assert_eq!(stats.modified, 0, "no files should be modified");
    assert_eq!(stats.deleted, 0, "no files should be deleted");
}

#[test]
fn incremental_git_uncommitted_changes() {
    let (_tmp, root) = git_indexed_fixture();

    // Make a change without committing — should still be detected
    let new_file = root.join("src").join("uncommitted.rs");
    fs::write(
        &new_file,
        "pub fn uncommitted_function() {}\n",
    )
    .expect("should write file");

    let stats = ns::indexer::run_incremental_index(&root, 1_048_576)
        .expect("incremental should succeed");

    assert!(
        stats.added >= 1,
        "should detect uncommitted added file, got {} added",
        stats.added
    );

    // Verify searchable
    let (results, _) =
        ns::searcher::query::execute_search(&root, "uncommitted_function", &opts(10))
            .expect("search should work");

    assert!(
        !results.is_empty(),
        "should find 'uncommitted_function' after incremental"
    );
}
