//! Git hook management for automatic incremental re-indexing.
//!
//! Unix-only: git hooks require a POSIX shell. This module uses
//! `std::os::unix::fs::PermissionsExt` for chmod and will not compile
//! on non-Unix platforms.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::cmd::HooksAction;

/// Marker comment used to identify ns-managed hook lines.
const NS_MARKER: &str = "# ns: auto-generated";

/// The hook payload: run incremental indexing in the background.
const NS_HOOK_LINE: &str = "ns index --incremental &";

/// Hook names that ns installs.
const HOOK_NAMES: &[&str] = &["post-commit", "post-merge", "post-checkout"];

pub fn run(action: &HooksAction) {
    match action {
        HooksAction::Install => install(),
        HooksAction::Remove => remove(),
    }
}

fn hooks_dir() -> Result<PathBuf, String> {
    let root = PathBuf::from(".")
        .canonicalize()
        .map_err(|e| format!("cannot resolve current directory: {}", e))?;

    let git_dir = root.join(".git");
    if !git_dir.exists() {
        return Err("not a git repository. Git hooks require a .git directory.".to_string());
    }

    Ok(git_dir.join("hooks"))
}

fn install() {
    let hooks_dir = match hooks_dir() {
        Ok(d) => d,
        Err(msg) => {
            eprintln!("error: {}", msg);
            std::process::exit(1);
        }
    };

    if let Err(e) = fs::create_dir_all(&hooks_dir) {
        eprintln!("error: cannot create hooks directory: {}", e);
        std::process::exit(1);
    }

    let mut installed = 0;
    let mut skipped = 0;

    for &hook_name in HOOK_NAMES {
        let hook_path = hooks_dir.join(hook_name);
        match install_hook(&hook_path, hook_name) {
            HookResult::Created => {
                eprintln!("installed {}", hook_name);
                installed += 1;
            }
            HookResult::Appended => {
                eprintln!("appended to existing {}", hook_name);
                installed += 1;
            }
            HookResult::AlreadyPresent => {
                eprintln!("{} already has ns hook", hook_name);
                skipped += 1;
            }
            HookResult::NotShellScript => {
                eprintln!(
                    "warning: {} exists but is not a shell script — skipping",
                    hook_name
                );
                skipped += 1;
            }
            HookResult::Error(msg) => {
                eprintln!("error: {}: {}", hook_name, msg);
                skipped += 1;
            }
        }
    }

    if installed > 0 {
        eprintln!(
            "Done. {} hook{} installed, {} skipped.",
            installed,
            if installed == 1 { "" } else { "s" },
            skipped
        );
    } else {
        eprintln!("No hooks installed ({} skipped).", skipped);
    }
}

enum HookResult {
    Created,
    Appended,
    AlreadyPresent,
    NotShellScript,
    Error(String),
}

fn install_hook(hook_path: &Path, _hook_name: &str) -> HookResult {
    if hook_path.exists() {
        // Read existing content
        let content = match fs::read_to_string(hook_path) {
            Ok(c) => c,
            Err(e) => return HookResult::Error(format!("cannot read: {}", e)),
        };

        // Already has our hook line?
        if content.contains(NS_HOOK_LINE) {
            return HookResult::AlreadyPresent;
        }

        // Check it's a shell script — must have a shell shebang
        if !is_shell_script(&content) {
            return HookResult::NotShellScript;
        }

        // Append our lines
        let appendix = format!("\n{}\n{}\n", NS_MARKER, NS_HOOK_LINE);
        if let Err(e) = fs::write(hook_path, format!("{}{}", content, appendix)) {
            return HookResult::Error(format!("cannot write: {}", e));
        }

        // Ensure executable bit is set (fs::write preserves perms on most Unix
        // filesystems, but be explicit for safety)
        if let Err(e) = make_executable(hook_path) {
            return HookResult::Error(format!("cannot set executable: {}", e));
        }

        HookResult::Appended
    } else {
        // Create new hook
        let content = format!("#!/bin/sh\n{}\n{}\n", NS_MARKER, NS_HOOK_LINE);
        if let Err(e) = fs::write(hook_path, content) {
            return HookResult::Error(format!("cannot write: {}", e));
        }

        // Make executable
        if let Err(e) = make_executable(hook_path) {
            return HookResult::Error(format!("cannot set executable: {}", e));
        }

        HookResult::Created
    }
}

/// Returns true if the file content looks like a shell script.
///
/// Checks for common shell shebangs: `/bin/sh`, `/bin/bash`, `/bin/zsh`,
/// and `/usr/bin/env sh|bash|zsh`. Handles flags (e.g., `#!/bin/sh -e`).
fn is_shell_script(content: &str) -> bool {
    let first_line = match content.lines().next() {
        Some(l) => l,
        None => return false,
    };
    if !first_line.starts_with("#!") {
        return false;
    }
    let shebang = first_line[2..].trim();

    // Extract the interpreter path (before any flags)
    let interpreter = shebang.split_whitespace().next().unwrap_or("");

    // Direct shell paths
    if matches!(
        interpreter,
        "/bin/sh"
            | "/bin/bash"
            | "/bin/zsh"
            | "/usr/bin/sh"
            | "/usr/bin/bash"
            | "/usr/bin/zsh"
    ) {
        return true;
    }

    // env-based: #!/usr/bin/env sh, #!/usr/bin/env bash, etc.
    if interpreter == "/usr/bin/env" {
        // The shell name is the next token after "env"
        let mut parts = shebang.split_whitespace();
        parts.next(); // skip "/usr/bin/env"
        if let Some(cmd) = parts.next() {
            return matches!(cmd, "sh" | "bash" | "zsh");
        }
    }
    false
}

fn make_executable(path: &Path) -> std::io::Result<()> {
    let metadata = fs::metadata(path)?;
    let mut perms = metadata.permissions();
    perms.set_mode(perms.mode() | 0o111);
    fs::set_permissions(path, perms)
}

fn remove() {
    let hooks_dir = match hooks_dir() {
        Ok(d) => d,
        Err(msg) => {
            eprintln!("error: {}", msg);
            std::process::exit(1);
        }
    };

    let mut removed = 0;

    for &hook_name in HOOK_NAMES {
        let hook_path = hooks_dir.join(hook_name);
        match remove_hook(&hook_path, hook_name) {
            RemoveResult::Deleted => {
                eprintln!("removed {}", hook_name);
                removed += 1;
            }
            RemoveResult::Cleaned => {
                eprintln!("removed ns lines from {}", hook_name);
                removed += 1;
            }
            RemoveResult::NotPresent => {}
            RemoveResult::Error(msg) => {
                eprintln!("error: {}: {}", hook_name, msg);
            }
        }
    }

    if removed > 0 {
        eprintln!(
            "Done. {} hook{} removed.",
            removed,
            if removed == 1 { "" } else { "s" }
        );
    } else {
        eprintln!("No ns hooks found to remove.");
    }
}

enum RemoveResult {
    Deleted,
    Cleaned,
    NotPresent,
    Error(String),
}

fn remove_hook(hook_path: &Path, _hook_name: &str) -> RemoveResult {
    if !hook_path.exists() {
        return RemoveResult::NotPresent;
    }

    let content = match fs::read_to_string(hook_path) {
        Ok(c) => c,
        Err(e) => return RemoveResult::Error(format!("cannot read: {}", e)),
    };

    // Check for either marker or hook line (handles orphaned markers too)
    if !content.contains(NS_HOOK_LINE) && !content.contains(NS_MARKER) {
        return RemoveResult::NotPresent;
    }

    // Remove our marker and hook lines
    let cleaned: Vec<&str> = content
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            trimmed != NS_MARKER && trimmed != NS_HOOK_LINE
        })
        .collect();

    // If only the shebang (or nothing meaningful) remains, delete the whole file
    let meaningful_lines: Vec<&&str> = cleaned
        .iter()
        .filter(|l| !l.trim().is_empty() && !l.starts_with("#!"))
        .collect();

    if meaningful_lines.is_empty() {
        if let Err(e) = fs::remove_file(hook_path) {
            return RemoveResult::Error(format!("cannot delete: {}", e));
        }
        RemoveResult::Deleted
    } else {
        // Rejoin, trim trailing blank lines (prevents accumulation from
        // install's leading "\n" separator), then add single trailing newline
        let joined = cleaned.join("\n");
        let trimmed = joined.trim_end();
        let new_content = format!("{}\n", trimmed);

        if let Err(e) = fs::write(hook_path, new_content) {
            return RemoveResult::Error(format!("cannot write: {}", e));
        }
        RemoveResult::Cleaned
    }
}
