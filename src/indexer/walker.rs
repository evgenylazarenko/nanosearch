use std::io::Read as _;
use std::path::Path;

use ignore::WalkBuilder;

use super::language::detect_language;

/// A file that has been read and is ready for indexing.
pub struct WalkedFile {
    /// Path relative to the repo root.
    pub rel_path: String,
    /// Full file content as a UTF-8 string.
    pub content: String,
    /// Detected language identifier, or `None` if unknown/unsupported.
    pub lang: Option<String>,
}

/// Walks the repository at `root`, returning indexable files.
///
/// Skips:
/// - Files ignored by `.gitignore`
/// - `.git/` and `.ns/` directories
/// - Binary files (null byte in first 512 bytes)
/// - Files larger than `max_file_size`
/// - Non-UTF-8 files
pub fn walk_repo(root: &Path, max_file_size: u64) -> Vec<WalkedFile> {
    let mut files = Vec::new();

    let walker = WalkBuilder::new(root)
        .follow_links(false)
        .hidden(false) // don't skip dotfiles (gitignore handles that)
        .filter_entry(|entry| {
            let name = entry.file_name().to_string_lossy();
            // Skip .git and .ns directories
            if entry.file_type().map_or(false, |ft| ft.is_dir()) {
                return name != ".git" && name != ".ns";
            }
            true
        })
        .build();

    for result in walker {
        let entry = match result {
            Ok(e) => e,
            Err(err) => {
                eprintln!("warning: walk error: {}", err);
                continue;
            }
        };

        // Only process files
        if !entry.file_type().map_or(false, |ft| ft.is_file()) {
            continue;
        }

        let path = entry.path();

        // Check file size
        let metadata = match path.metadata() {
            Ok(m) => m,
            Err(err) => {
                eprintln!("warning: cannot stat {}: {}", path.display(), err);
                continue;
            }
        };
        if metadata.len() > max_file_size {
            continue;
        }

        // Binary check: read only the first 512 bytes before committing to a full read.
        // This avoids loading a large binary file entirely into memory.
        let mut file_handle = match std::fs::File::open(path) {
            Ok(f) => f,
            Err(err) => {
                eprintln!("warning: cannot open {}: {}", path.display(), err);
                continue;
            }
        };
        let mut header = [0u8; 512];
        let header_len = match file_handle.read(&mut header) {
            Ok(n) => n,
            Err(err) => {
                eprintln!("warning: cannot read {}: {}", path.display(), err);
                continue;
            }
        };
        if header[..header_len].contains(&0) {
            continue;
        }
        drop(file_handle);

        // Full read (now that we know it's likely text)
        let raw = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(err) => {
                eprintln!("warning: cannot read {}: {}", path.display(), err);
                continue;
            }
        };

        // UTF-8 check
        let content = match String::from_utf8(raw) {
            Ok(s) => s,
            Err(_) => {
                eprintln!("warning: skipping non-UTF-8 file: {}", path.display());
                continue;
            }
        };

        // Compute relative path
        let rel_path = match path.strip_prefix(root) {
            Ok(rel) => rel.to_string_lossy().to_string(),
            Err(_) => path.to_string_lossy().to_string(),
        };

        let lang = detect_language(path).map(|s| s.to_string());

        files.push(WalkedFile {
            rel_path,
            content,
            lang,
        });
    }

    files
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn walks_fixture_repo() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/sample_repo");

        let files = walk_repo(&fixture, 1_048_576);

        // Should find all source files + README + config.json
        assert!(
            files.len() >= 6,
            "expected at least 6 files, got {}",
            files.len()
        );

        let paths: Vec<&str> = files.iter().map(|f| f.rel_path.as_str()).collect();
        assert!(paths.iter().any(|p| p.contains("event_store.rs")));
        assert!(paths.iter().any(|p| p.contains("models.py")));
        assert!(paths.iter().any(|p| p.contains("server.go")));

        // Language detection works
        let rs_file = files.iter().find(|f| f.rel_path.contains("event_store.rs")).unwrap();
        assert_eq!(rs_file.lang.as_deref(), Some("rust"));

        let md_file = files.iter().find(|f| f.rel_path.contains("README.md")).unwrap();
        assert_eq!(md_file.lang, None);
    }

    #[test]
    fn skips_large_files() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/sample_repo");

        // Set max file size to 100 bytes — should skip most files
        let files = walk_repo(&fixture, 100);
        assert!(
            files.len() < 8,
            "expected fewer files with 100-byte limit, got {}",
            files.len()
        );
    }
}
