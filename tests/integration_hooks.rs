//! Integration tests for `ns hooks install` and `ns hooks remove`.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

/// Creates a tempdir with a `.git/hooks` directory (minimal git repo structure).
fn git_repo() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().expect("should create tempdir");
    let root = tmp.path().to_path_buf();
    fs::create_dir_all(root.join(".git/hooks")).expect("should create .git/hooks");
    (tmp, root)
}

/// Creates a tempdir with NO `.git` directory.
fn non_git_dir() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().expect("should create tempdir");
    let root = tmp.path().to_path_buf();
    (tmp, root)
}

fn ns_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_ns"))
}

const HOOK_NAMES: &[&str] = &["post-commit", "post-merge", "post-checkout"];

// ── Install tests ─────────────────────────────────────────────────────────────

#[test]
fn install_creates_all_three_hooks() {
    let (_tmp, root) = git_repo();

    let output = std::process::Command::new(ns_binary())
        .args(["hooks", "install"])
        .current_dir(&root)
        .output()
        .expect("should run ns");

    assert!(output.status.success(), "should exit 0");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("installed post-commit"), "stderr: {}", stderr);
    assert!(stderr.contains("installed post-merge"), "stderr: {}", stderr);
    assert!(stderr.contains("installed post-checkout"), "stderr: {}", stderr);

    for &hook in HOOK_NAMES {
        let hook_path = root.join(".git/hooks").join(hook);
        assert!(hook_path.exists(), "{} should exist", hook);

        let content = fs::read_to_string(&hook_path).expect("should read hook");
        assert!(content.starts_with("#!/bin/sh"), "{} should have shebang", hook);
        assert!(
            content.contains("ns index --incremental &"),
            "{} should have ns line",
            hook
        );
        assert!(
            content.contains("# ns: auto-generated"),
            "{} should have marker",
            hook
        );

        // Check executable
        let perms = fs::metadata(&hook_path).expect("metadata").permissions();
        assert!(perms.mode() & 0o111 != 0, "{} should be executable", hook);
    }
}

#[test]
fn install_is_idempotent() {
    let (_tmp, root) = git_repo();

    // Install twice
    for _ in 0..2 {
        let output = std::process::Command::new(ns_binary())
            .args(["hooks", "install"])
            .current_dir(&root)
            .output()
            .expect("should run ns");
        assert!(output.status.success());
    }

    // Each hook should contain ns line exactly once
    for &hook in HOOK_NAMES {
        let content = fs::read_to_string(root.join(".git/hooks").join(hook)).expect("read");
        let count = content.matches("ns index --incremental &").count();
        assert_eq!(count, 1, "{} should have ns line exactly once, got {}", hook, count);
    }
}

#[test]
fn install_appends_to_existing_shell_hook() {
    let (_tmp, root) = git_repo();

    // Create an existing post-commit hook
    let hook_path = root.join(".git/hooks/post-commit");
    fs::write(&hook_path, "#!/bin/sh\necho 'existing hook'\n").expect("write");
    // Make executable
    let mut perms = fs::metadata(&hook_path).expect("metadata").permissions();
    perms.set_mode(perms.mode() | 0o111);
    fs::set_permissions(&hook_path, perms).expect("chmod");

    let output = std::process::Command::new(ns_binary())
        .args(["hooks", "install"])
        .current_dir(&root)
        .output()
        .expect("should run ns");

    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("appended to existing post-commit"),
        "should append, got: {}",
        stderr
    );

    let content = fs::read_to_string(&hook_path).expect("read");
    assert!(content.contains("echo 'existing hook'"), "should preserve existing content");
    assert!(content.contains("ns index --incremental &"), "should have ns line");
}

#[test]
fn install_warns_for_non_shell_script() {
    let (_tmp, root) = git_repo();

    // Create a Python hook — not a shell script
    let hook_path = root.join(".git/hooks/post-commit");
    fs::write(&hook_path, "#!/usr/bin/env python3\nprint('hi')\n").expect("write");

    let output = std::process::Command::new(ns_binary())
        .args(["hooks", "install"])
        .current_dir(&root)
        .output()
        .expect("should run ns");

    // Should still succeed (other hooks get installed)
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not a shell script"),
        "should warn about non-shell hook, got: {}",
        stderr
    );

    // The python hook should be untouched
    let content = fs::read_to_string(&hook_path).expect("read");
    assert!(!content.contains("ns index"), "should not modify non-shell hook");
}

#[test]
fn install_warns_for_binary_hook() {
    let (_tmp, root) = git_repo();

    // Create a binary-like hook (no shebang)
    let hook_path = root.join(".git/hooks/post-commit");
    fs::write(&hook_path, b"ELF\x00\x00binary stuff").expect("write");

    let output = std::process::Command::new(ns_binary())
        .args(["hooks", "install"])
        .current_dir(&root)
        .output()
        .expect("should run ns");

    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not a shell script"),
        "should warn about non-shell hook, got: {}",
        stderr
    );
}

#[test]
fn install_fails_in_non_git_dir() {
    let (_tmp, root) = non_git_dir();

    let output = std::process::Command::new(ns_binary())
        .args(["hooks", "install"])
        .current_dir(&root)
        .output()
        .expect("should run ns");

    assert!(!output.status.success(), "should exit non-zero");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not a git repository"),
        "should mention not a git repo, got: {}",
        stderr
    );
}

// ── Remove tests ──────────────────────────────────────────────────────────────

#[test]
fn remove_deletes_ns_only_hooks() {
    let (_tmp, root) = git_repo();

    // Install first
    std::process::Command::new(ns_binary())
        .args(["hooks", "install"])
        .current_dir(&root)
        .output()
        .expect("install");

    // Remove
    let output = std::process::Command::new(ns_binary())
        .args(["hooks", "remove"])
        .current_dir(&root)
        .output()
        .expect("should run ns");

    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("removed post-commit"), "stderr: {}", stderr);

    // Hook files should be gone (they only contained ns content)
    for &hook in HOOK_NAMES {
        assert!(
            !root.join(".git/hooks").join(hook).exists(),
            "{} should be deleted",
            hook
        );
    }
}

#[test]
fn remove_preserves_existing_hook_content() {
    let (_tmp, root) = git_repo();

    // Create existing hook, then install ns
    let hook_path = root.join(".git/hooks/post-commit");
    fs::write(&hook_path, "#!/bin/sh\necho 'keep me'\n").expect("write");
    let mut perms = fs::metadata(&hook_path).expect("metadata").permissions();
    perms.set_mode(perms.mode() | 0o111);
    fs::set_permissions(&hook_path, perms).expect("chmod");

    std::process::Command::new(ns_binary())
        .args(["hooks", "install"])
        .current_dir(&root)
        .output()
        .expect("install");

    // Now remove
    let output = std::process::Command::new(ns_binary())
        .args(["hooks", "remove"])
        .current_dir(&root)
        .output()
        .expect("should run ns");

    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("removed ns lines from post-commit"),
        "should clean, got: {}",
        stderr
    );

    // Hook file should still exist with original content
    let content = fs::read_to_string(&hook_path).expect("read");
    assert!(content.contains("echo 'keep me'"), "should preserve original lines");
    assert!(!content.contains("ns index --incremental"), "ns lines should be gone");
    assert!(!content.contains("# ns: auto-generated"), "marker should be gone");
}

#[test]
fn remove_is_idempotent() {
    let (_tmp, root) = git_repo();

    // Remove without having installed — should be a no-op
    let output = std::process::Command::new(ns_binary())
        .args(["hooks", "remove"])
        .current_dir(&root)
        .output()
        .expect("should run ns");

    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("No ns hooks found"),
        "should report nothing to remove, got: {}",
        stderr
    );
}

#[test]
fn remove_fails_in_non_git_dir() {
    let (_tmp, root) = non_git_dir();

    let output = std::process::Command::new(ns_binary())
        .args(["hooks", "remove"])
        .current_dir(&root)
        .output()
        .expect("should run ns");

    assert!(!output.status.success(), "should exit non-zero");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not a git repository"),
        "should mention not a git repo, got: {}",
        stderr
    );
}

// ── Additional edge-case tests ────────────────────────────────────────────────

#[test]
fn install_appends_to_hook_without_trailing_newline() {
    let (_tmp, root) = git_repo();

    // Existing hook with NO trailing newline
    let hook_path = root.join(".git/hooks/post-commit");
    fs::write(&hook_path, "#!/bin/sh\necho 'no newline at end'").expect("write");
    let mut perms = fs::metadata(&hook_path).expect("metadata").permissions();
    perms.set_mode(perms.mode() | 0o111);
    fs::set_permissions(&hook_path, perms).expect("chmod");

    let output = std::process::Command::new(ns_binary())
        .args(["hooks", "install"])
        .current_dir(&root)
        .output()
        .expect("should run ns");

    assert!(output.status.success());

    let content = fs::read_to_string(&hook_path).expect("read");
    assert!(
        content.contains("echo 'no newline at end'"),
        "should preserve existing content"
    );
    assert!(
        content.contains("ns index --incremental &"),
        "should have ns line"
    );
    // The ns block should be separated from existing content
    assert!(
        content.contains("# ns: auto-generated"),
        "should have marker"
    );
}

#[test]
fn install_recognizes_shebang_with_flags() {
    let (_tmp, root) = git_repo();

    // Hook with #!/bin/sh -e (common in real git hooks)
    let hook_path = root.join(".git/hooks/post-commit");
    fs::write(&hook_path, "#!/bin/sh -e\necho 'strict mode'\n").expect("write");
    let mut perms = fs::metadata(&hook_path).expect("metadata").permissions();
    perms.set_mode(perms.mode() | 0o111);
    fs::set_permissions(&hook_path, perms).expect("chmod");

    let output = std::process::Command::new(ns_binary())
        .args(["hooks", "install"])
        .current_dir(&root)
        .output()
        .expect("should run ns");

    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("appended to existing post-commit"),
        "should recognize #!/bin/sh -e as shell script, got: {}",
        stderr
    );

    let content = fs::read_to_string(&hook_path).expect("read");
    assert!(content.contains("ns index --incremental &"));
}

#[test]
fn install_remove_install_roundtrip_no_blank_accumulation() {
    let (_tmp, root) = git_repo();

    // Create existing hook
    let hook_path = root.join(".git/hooks/post-commit");
    fs::write(&hook_path, "#!/bin/sh\necho 'user hook'\n").expect("write");
    let mut perms = fs::metadata(&hook_path).expect("metadata").permissions();
    perms.set_mode(perms.mode() | 0o111);
    fs::set_permissions(&hook_path, perms).expect("chmod");

    // Run 3 cycles of install/remove
    for _ in 0..3 {
        std::process::Command::new(ns_binary())
            .args(["hooks", "install"])
            .current_dir(&root)
            .output()
            .expect("install");

        std::process::Command::new(ns_binary())
            .args(["hooks", "remove"])
            .current_dir(&root)
            .output()
            .expect("remove");
    }

    // After 3 cycles, the file should have no accumulated blank lines
    let content = fs::read_to_string(&hook_path).expect("read");
    assert!(
        !content.contains("ns index"),
        "ns lines should be gone after remove"
    );

    // Count blank lines — should be 0 (just shebang + user line)
    let blank_count = content.lines().filter(|l| l.trim().is_empty()).count();
    assert!(
        blank_count == 0,
        "should not accumulate blank lines after install/remove cycles, got {} blanks in: {:?}",
        blank_count,
        content
    );
}
