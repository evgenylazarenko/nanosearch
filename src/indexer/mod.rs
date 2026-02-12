pub mod language;
pub mod symbols;
pub mod walker;
pub mod writer;

use std::path::Path;

use crate::error::NsError;
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
