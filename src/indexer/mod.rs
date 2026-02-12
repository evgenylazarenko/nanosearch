pub mod incremental;
pub mod language;
pub mod symbols;
pub mod walker;
pub mod writer;

use std::path::Path;

use crate::error::NsError;
use incremental::{run_incremental, IncrementalStats};
use walker::walk_repo;
use writer::build_index;

/// Runs a full (non-incremental) index of the repository at `root`.
pub fn run_full_index(root: &Path, max_file_size: u64) -> Result<usize, NsError> {
    let files = walk_repo(root, max_file_size);
    if files.is_empty() {
        eprintln!("No indexable files found.");
        return Ok(0);
    }
    build_index(root, &files)
}

/// Runs an incremental index update on the repository at `root`.
///
/// Requires an existing index (created by `run_full_index`).
/// Detects changes via git diff (preferred) or mtime fallback,
/// then applies adds/modifies/deletes to the existing index.
pub fn run_incremental_index(
    root: &Path,
    max_file_size: u64,
) -> Result<IncrementalStats, NsError> {
    run_incremental(root, max_file_size)
}
