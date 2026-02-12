use std::path::PathBuf;

use crate::cmd::IndexArgs;
use crate::error::NsError;
use crate::indexer;
use crate::indexer::writer::check_gitignore_warning;

pub fn run(args: &IndexArgs) {
    let root = args
        .root
        .clone()
        .unwrap_or_else(|| PathBuf::from("."));

    let root = match root.canonicalize() {
        Ok(p) => p,
        Err(err) => {
            eprintln!("error: cannot resolve root path '{}': {}", root.display(), err);
            std::process::exit(1);
        }
    };

    if args.incremental {
        run_incremental(&root, args.max_file_size);
    } else {
        run_full(&root, args.max_file_size);
    }
}

fn run_full(root: &std::path::Path, max_file_size: u64) {
    match indexer::run_full_index(root, max_file_size) {
        Ok(None) => {
            eprintln!("No indexable files found.");
        }
        Ok(Some(stats)) => {
            eprintln!("Indexed {} files in {}ms", stats.file_count, stats.elapsed_ms);
            check_gitignore_warning(root);
        }
        Err(err) => {
            match &err {
                _ if err.is_lock_error() => {
                    eprintln!("error: index is locked by another process.");
                }
                NsError::Io(e) => {
                    eprintln!("error: I/O failure during indexing: {}", e);
                }
                NsError::Tantivy(e) => {
                    eprintln!("error: index engine failure: {}", e);
                }
                NsError::Json(e) => {
                    eprintln!("error: failed to write index metadata: {}", e);
                }
                _ => {
                    eprintln!("error: indexing failed: {}", err);
                }
            }
            std::process::exit(1);
        }
    }
}

fn run_incremental(root: &std::path::Path, max_file_size: u64) {
    match indexer::run_incremental_index(root, max_file_size) {
        Ok(stats) => {
            if stats.added == 0 && stats.modified == 0 && stats.deleted == 0 {
                eprintln!("Index is up to date.");
            } else {
                eprintln!(
                    "Incremental update: {} added, {} modified, {} deleted in {}ms",
                    stats.added, stats.modified, stats.deleted, stats.elapsed_ms
                );
            }
            check_gitignore_warning(root);
        }
        Err(err) => {
            match &err {
                NsError::Io(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    eprintln!("error: no index found. Run 'ns index' first (without --incremental).");
                }
                NsError::SchemaVersionMismatch { .. } => {
                    eprintln!(
                        "error: index was built with an older version of ns. Run 'ns index' to rebuild."
                    );
                }
                _ if err.is_lock_error() => {
                    eprintln!("error: index is locked by another process.");
                }
                NsError::Tantivy(e) => {
                    eprintln!("error: index engine failure: {}", e);
                }
                NsError::Io(e) => {
                    eprintln!("error: I/O failure during incremental indexing: {}", e);
                }
                NsError::Json(e) => {
                    eprintln!("error: failed to write index metadata: {}", e);
                }
                _ => {
                    eprintln!("error: incremental indexing failed: {}", err);
                }
            }
            std::process::exit(1);
        }
    }
}
