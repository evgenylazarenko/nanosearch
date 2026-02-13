use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::time::Instant;

use tantivy::schema::Value;
use tantivy::{IndexWriter, ReloadPolicy, TantivyDocument, Term};

use crate::error::NsError;
use crate::schema::{
    content_field, lang_field, path_field, symbols_field, symbols_raw_field,
};

use super::language::detect_language;
use super::symbols::extract_symbols;
use super::walker::walk_repo;
use super::writer::{
    dir_size, get_git_commit, open_index, utc_timestamp_iso8601, IndexMeta,
    SCHEMA_VERSION,
};

/// Summary of an incremental index operation.
pub struct IncrementalStats {
    pub added: usize,
    pub modified: usize,
    pub deleted: usize,
    pub elapsed_ms: u64,
}

/// Three lists of relative paths describing what changed since the last index.
struct ChangeSet {
    added: Vec<String>,
    modified: Vec<String>,
    deleted: Vec<String>,
}

/// Runs an incremental index update on the repository at `root`.
///
/// 1. Opens the existing index and reads meta.json
/// 2. Detects changes (git-based or mtime-based fallback)
/// 3. Deletes documents for deleted/modified files
/// 4. Re-indexes modified and added files
/// 5. Commits and updates meta.json
pub fn run_incremental(
    root: &Path,
    max_file_size: u64,
) -> Result<IncrementalStats, NsError> {
    let (index, meta) = open_index(root)?;

    let changes = detect_changes(root, &meta, &index, max_file_size)?;

    let total_changes = changes.added.len() + changes.modified.len() + changes.deleted.len();
    if total_changes == 0 {
        return Ok(IncrementalStats {
            added: 0,
            modified: 0,
            deleted: 0,
            elapsed_ms: 0,
        });
    }

    let schema = index.schema();
    let content_f = content_field(&schema);
    let symbols_f = symbols_field(&schema);
    let symbols_raw_f = symbols_raw_field(&schema);
    let path_f = path_field(&schema);
    let lang_f = lang_field(&schema);

    let mut writer: IndexWriter = index.writer(50_000_000)?;

    let start = Instant::now();

    // Delete documents for deleted files
    for rel_path in &changes.deleted {
        writer.delete_term(Term::from_field_text(path_f, rel_path));
    }

    // Delete then re-index modified files
    for rel_path in &changes.modified {
        writer.delete_term(Term::from_field_text(path_f, rel_path));
        if let Some(doc) = build_document(root, rel_path, content_f, symbols_f, symbols_raw_f, path_f, lang_f) {
            writer.add_document(doc)?;
        }
    }

    // Index added files
    for rel_path in &changes.added {
        if let Some(doc) = build_document(root, rel_path, content_f, symbols_f, symbols_raw_f, path_f, lang_f) {
            writer.add_document(doc)?;
        }
    }

    writer.commit()?;

    let elapsed_ms = start.elapsed().as_millis() as u64;

    // Count total documents in the index after commit
    let reader = index
        .reader_builder()
        .reload_policy(ReloadPolicy::Manual)
        .try_into()?;
    let searcher = reader.searcher();
    let file_count = searcher.num_docs() as usize;

    // Calculate index size
    let index_dir = root.join(".ns").join("index");
    let index_size = dir_size(&index_dir);

    // Update meta.json
    let git_commit = get_git_commit(root);
    let new_meta = IndexMeta {
        schema_version: SCHEMA_VERSION,
        indexed_at: utc_timestamp_iso8601(),
        git_commit,
        file_count,
        index_size_bytes: index_size,
    };

    let meta_path = root.join(".ns").join("meta.json");
    let meta_json = serde_json::to_string_pretty(&new_meta)?;
    fs::write(&meta_path, &meta_json)?;

    let stats = IncrementalStats {
        added: changes.added.len(),
        modified: changes.modified.len(),
        deleted: changes.deleted.len(),
        elapsed_ms,
    };

    Ok(stats)
}

/// Reads the set of all file paths currently in the tantivy index.
fn get_indexed_paths(index: &tantivy::Index) -> Result<HashSet<String>, NsError> {
    let reader = index
        .reader_builder()
        .reload_policy(ReloadPolicy::Manual)
        .try_into()?;
    let searcher = reader.searcher();
    let schema = index.schema();
    let path_f = path_field(&schema);

    let mut paths = HashSet::new();
    for segment_reader in searcher.segment_readers() {
        let store_reader = segment_reader.get_store_reader(1)?;
        for doc_id in 0..segment_reader.num_docs() {
            if let Ok(doc) = store_reader.get::<TantivyDocument>(doc_id) {
                if let Some(val) = doc.get_first(path_f) {
                    if let Some(path_str) = val.as_str() {
                        paths.insert(path_str.to_string());
                    }
                }
            }
        }
    }
    Ok(paths)
}

/// Detects changes since the last index using git diff (preferred) or mtime fallback.
fn detect_changes(
    root: &Path,
    meta: &IndexMeta,
    index: &tantivy::Index,
    max_file_size: u64,
) -> Result<ChangeSet, NsError> {
    // Try git-based detection first
    if let Some(ref old_commit) = meta.git_commit {
        if let Some(current_commit) = get_git_commit(root) {
            let indexed_paths = get_indexed_paths(index)?;
            if *old_commit == current_commit {
                // Same commit — check for uncommitted changes via working tree diff
                return detect_changes_git_uncommitted(
                    root, max_file_size, &indexed_paths, &meta.indexed_at,
                );
            }
            return detect_changes_git(
                root, old_commit, &current_commit, max_file_size, &indexed_paths, &meta.indexed_at,
            );
        }
    }

    // Fallback: mtime-based detection
    detect_changes_mtime(root, meta, index, max_file_size)
}

/// Detects changes using `git diff --name-status` between two commits,
/// plus any uncommitted working tree changes.
fn detect_changes_git(
    root: &Path,
    old_commit: &str,
    current_commit: &str,
    max_file_size: u64,
    indexed_paths: &HashSet<String>,
    indexed_at: &str,
) -> Result<ChangeSet, NsError> {
    // Get committed changes between old and current commit
    let mut changes = parse_git_diff(root, old_commit, current_commit)?;

    // Also check for uncommitted working tree changes (staged + unstaged)
    let working_changes =
        detect_changes_git_uncommitted(root, max_file_size, indexed_paths, indexed_at)?;

    // Merge working tree changes into committed changes
    merge_changesets(&mut changes, working_changes);

    // Filter: only include files that would actually be walked (exist, not binary, etc.)
    filter_changeset(root, &mut changes, max_file_size);

    Ok(changes)
}

/// Detects uncommitted changes (both staged and unstaged) against HEAD.
///
/// `indexed_paths` is the set of file paths already in the tantivy index.
/// Untracked files already in the index are skipped (or classified as modified
/// if their mtime is newer than `indexed_at`), preventing duplicate document
/// insertion on repeated incremental runs.
fn detect_changes_git_uncommitted(
    root: &Path,
    max_file_size: u64,
    indexed_paths: &HashSet<String>,
    indexed_at: &str,
) -> Result<ChangeSet, NsError> {
    // git diff --name-status HEAD (working tree vs HEAD, includes staged)
    let output = std::process::Command::new("git")
        .args(["diff", "--name-status", "HEAD"])
        .current_dir(root)
        .output()
        .map_err(|e| NsError::Io(e))?;

    if !output.status.success() {
        // If git diff fails (e.g., initial commit with no HEAD), return empty
        return Ok(ChangeSet {
            added: Vec::new(),
            modified: Vec::new(),
            deleted: Vec::new(),
        });
    }

    let mut changes = parse_name_status_output(&String::from_utf8_lossy(&output.stdout));

    // Also check for untracked files
    let untracked_output = std::process::Command::new("git")
        .args(["ls-files", "--others", "--exclude-standard"])
        .current_dir(root)
        .output()
        .map_err(|e| NsError::Io(e))?;

    if untracked_output.status.success() {
        let indexed_time = parse_iso8601_to_system_time(indexed_at);
        let untracked = String::from_utf8_lossy(&untracked_output.stdout);
        for line in untracked.lines() {
            let path = line.trim();
            if path.is_empty() || changes.added.contains(&path.to_string()) {
                continue;
            }
            if indexed_paths.contains(path) {
                // Already in the index — check if it was modified since last index
                if let Some(ref idx_time) = indexed_time {
                    let abs_path = root.join(path);
                    if let Ok(file_meta) = abs_path.metadata() {
                        if let Ok(mtime) = file_meta.modified() {
                            if mtime > *idx_time {
                                changes.modified.push(path.to_string());
                            }
                        }
                    }
                }
                // If mtime is not newer, skip — already indexed and up to date
            } else {
                // Not in index — genuinely new file
                changes.added.push(path.to_string());
            }
        }
    }

    filter_changeset(root, &mut changes, max_file_size);

    Ok(changes)
}

/// Parses `git diff --name-status` output between two refs.
fn parse_git_diff(
    root: &Path,
    old_ref: &str,
    new_ref: &str,
) -> Result<ChangeSet, NsError> {
    let output = std::process::Command::new("git")
        .args(["diff", "--name-status", old_ref, new_ref])
        .current_dir(root)
        .output()
        .map_err(|e| NsError::Io(e))?;

    if !output.status.success() {
        return Ok(ChangeSet {
            added: Vec::new(),
            modified: Vec::new(),
            deleted: Vec::new(),
        });
    }

    Ok(parse_name_status_output(&String::from_utf8_lossy(&output.stdout)))
}

/// Parses the output of `git diff --name-status` into a ChangeSet.
///
/// Format: `<status>\t<path>` per line
/// Status codes: A = added, M = modified, D = deleted, R = renamed (old\tnew)
fn parse_name_status_output(output: &str) -> ChangeSet {
    let mut added = Vec::new();
    let mut modified = Vec::new();
    let mut deleted = Vec::new();

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.splitn(3, '\t').collect();
        if parts.len() < 2 {
            continue;
        }

        let status = parts[0];
        let path = parts[1].to_string();

        match status.chars().next() {
            Some('A') => added.push(path),
            Some('M') => modified.push(path),
            Some('D') => deleted.push(path),
            Some('R') => {
                // Renamed: old path is deleted, new path is added
                deleted.push(path);
                if parts.len() >= 3 {
                    added.push(parts[2].to_string());
                }
            }
            _ => {
                // Other statuses (C = copied, T = type change) — treat as modified
                modified.push(path);
            }
        }
    }

    ChangeSet { added, modified, deleted }
}

/// Merges `other` into `base`, deduplicating paths.
fn merge_changesets(base: &mut ChangeSet, other: ChangeSet) {
    let existing: HashSet<String> = base
        .added
        .iter()
        .chain(base.modified.iter())
        .chain(base.deleted.iter())
        .cloned()
        .collect();

    for path in other.added {
        if !existing.contains(&path) {
            base.added.push(path);
        }
    }
    for path in other.modified {
        if !existing.contains(&path) {
            base.modified.push(path);
        }
    }
    for path in other.deleted {
        if !existing.contains(&path) {
            base.deleted.push(path);
        }
    }
}

/// Filters a changeset to remove paths that shouldn't be indexed
/// (e.g., .ns/ directory, .git/, files that no longer exist for added/modified).
fn filter_changeset(root: &Path, changes: &mut ChangeSet, max_file_size: u64) {
    let should_skip = |path: &str| -> bool {
        path.starts_with(".ns/")
            || path.starts_with(".ns\\")
            || path.starts_with(".git/")
            || path.starts_with(".git\\")
            || path == ".ns"
            || path == ".git"
    };

    // For added/modified files: must exist and be indexable
    let is_indexable = |rel_path: &str| -> bool {
        if should_skip(rel_path) {
            return false;
        }
        let abs_path = root.join(rel_path);
        if !abs_path.is_file() {
            return false;
        }
        // Check file size
        if let Ok(meta) = abs_path.metadata() {
            if meta.len() > max_file_size {
                return false;
            }
        }
        // Check for binary content (null bytes in first 512 bytes)
        if let Ok(raw) = fs::read(&abs_path) {
            let check_len = raw.len().min(512);
            if raw[..check_len].contains(&0) {
                return false;
            }
            // UTF-8 check
            if String::from_utf8(raw).is_err() {
                return false;
            }
        } else {
            return false;
        }
        true
    };

    changes.added.retain(|p| is_indexable(p));
    changes.modified.retain(|p| is_indexable(p));
    changes.deleted.retain(|p| !should_skip(p));
}

/// Detects changes using file mtime comparison against `meta.indexed_at`.
///
/// Used when git is not available or git_commit is not set.
fn detect_changes_mtime(
    root: &Path,
    meta: &IndexMeta,
    index: &tantivy::Index,
    max_file_size: u64,
) -> Result<ChangeSet, NsError> {
    let indexed_at = parse_iso8601_to_system_time(&meta.indexed_at);

    // Walk all current files
    let current_files = walk_repo(root, max_file_size);
    let current_paths: HashSet<String> = current_files
        .iter()
        .map(|f| f.rel_path.clone())
        .collect();

    let indexed_paths = get_indexed_paths(index)?;

    let mut added = Vec::new();
    let mut modified = Vec::new();
    let mut deleted = Vec::new();

    // Files in current walk but not in index → added
    // Files in both → check mtime for modified
    for file in &current_files {
        if !indexed_paths.contains(&file.rel_path) {
            added.push(file.rel_path.clone());
        } else if let Some(ref indexed_time) = indexed_at {
            let abs_path = root.join(&file.rel_path);
            if let Ok(file_meta) = abs_path.metadata() {
                if let Ok(mtime) = file_meta.modified() {
                    if mtime > *indexed_time {
                        modified.push(file.rel_path.clone());
                    }
                }
            }
        }
    }

    // Files in index but not in current walk → deleted
    for path in &indexed_paths {
        if !current_paths.contains(path) {
            deleted.push(path.clone());
        }
    }

    Ok(ChangeSet { added, modified, deleted })
}

/// Builds a tantivy document for a single file.
///
/// Returns `None` if the file cannot be read or is not indexable.
fn build_document(
    root: &Path,
    rel_path: &str,
    content_f: tantivy::schema::Field,
    symbols_f: tantivy::schema::Field,
    symbols_raw_f: tantivy::schema::Field,
    path_f: tantivy::schema::Field,
    lang_f: tantivy::schema::Field,
) -> Option<TantivyDocument> {
    let abs_path = root.join(rel_path);
    let content = fs::read_to_string(&abs_path).ok()?;
    let lang = detect_language(&abs_path).map(|s| s.to_string());

    let symbol_names = lang
        .as_deref()
        .map(|l| extract_symbols(l, content.as_bytes()))
        .unwrap_or_default();

    let mut doc = TantivyDocument::new();
    doc.add_text(content_f, &content);
    doc.add_text(symbols_f, &symbol_names.join(" "));
    doc.add_text(symbols_raw_f, &symbol_names.join("|"));
    doc.add_text(path_f, rel_path);
    if let Some(ref lang_str) = lang {
        doc.add_text(lang_f, lang_str);
    }

    Some(doc)
}

/// Parses an ISO 8601 timestamp string to SystemTime.
fn parse_iso8601_to_system_time(s: &str) -> Option<std::time::SystemTime> {
    // Parse format: "2025-02-11T14:30:00Z"
    let parts: Vec<&str> = s.split('T').collect();
    if parts.len() != 2 {
        return None;
    }
    let date_parts: Vec<u64> = parts[0].split('-').filter_map(|p| p.parse().ok()).collect();
    let time_str = parts[1].trim_end_matches('Z');
    let time_parts: Vec<u64> = time_str.split(':').filter_map(|p| p.parse().ok()).collect();

    if date_parts.len() != 3 || time_parts.len() != 3 {
        return None;
    }

    let (year, month, day) = (date_parts[0], date_parts[1], date_parts[2]);
    let (hour, min, sec) = (time_parts[0], time_parts[1], time_parts[2]);

    // Convert to days since epoch using civil calendar algorithm
    // Ref: http://howardhinnant.github.io/date_algorithms.html
    let y = if month <= 2 { year - 1 } else { year };
    let era = y / 400;
    let yoe = y - era * 400;
    let m = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * m + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;

    let total_secs = days * 86400 + hour * 3600 + min * 60 + sec;
    let duration = std::time::Duration::from_secs(total_secs);
    Some(std::time::UNIX_EPOCH + duration)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_name_status_added() {
        let output = "A\tsrc/new_file.rs\n";
        let changes = parse_name_status_output(output);
        assert_eq!(changes.added, vec!["src/new_file.rs"]);
        assert!(changes.modified.is_empty());
        assert!(changes.deleted.is_empty());
    }

    #[test]
    fn parse_name_status_modified() {
        let output = "M\tsrc/existing.rs\n";
        let changes = parse_name_status_output(output);
        assert!(changes.added.is_empty());
        assert_eq!(changes.modified, vec!["src/existing.rs"]);
        assert!(changes.deleted.is_empty());
    }

    #[test]
    fn parse_name_status_deleted() {
        let output = "D\tsrc/old_file.rs\n";
        let changes = parse_name_status_output(output);
        assert!(changes.added.is_empty());
        assert!(changes.modified.is_empty());
        assert_eq!(changes.deleted, vec!["src/old_file.rs"]);
    }

    #[test]
    fn parse_name_status_renamed() {
        let output = "R100\tsrc/old.rs\tsrc/new.rs\n";
        let changes = parse_name_status_output(output);
        assert_eq!(changes.added, vec!["src/new.rs"]);
        assert!(changes.modified.is_empty());
        assert_eq!(changes.deleted, vec!["src/old.rs"]);
    }

    #[test]
    fn parse_name_status_mixed() {
        let output = "A\tsrc/added.rs\nM\tsrc/modified.rs\nD\tsrc/deleted.rs\n";
        let changes = parse_name_status_output(output);
        assert_eq!(changes.added, vec!["src/added.rs"]);
        assert_eq!(changes.modified, vec!["src/modified.rs"]);
        assert_eq!(changes.deleted, vec!["src/deleted.rs"]);
    }

    #[test]
    fn parse_iso8601_roundtrip() {
        let ts = "2025-02-11T14:30:00Z";
        let sys_time = parse_iso8601_to_system_time(ts);
        assert!(sys_time.is_some(), "should parse valid ISO 8601 timestamp");
    }

    #[test]
    fn parse_iso8601_invalid() {
        assert!(parse_iso8601_to_system_time("not-a-date").is_none());
        assert!(parse_iso8601_to_system_time("").is_none());
    }
}
