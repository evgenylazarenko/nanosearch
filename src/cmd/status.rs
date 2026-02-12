use std::path::PathBuf;

use crate::error::NsError;
use crate::indexer::writer::{read_meta, SCHEMA_VERSION};

pub fn run() {
    let root = match PathBuf::from(".").canonicalize() {
        Ok(p) => p,
        Err(err) => {
            eprintln!("error: cannot resolve current directory: {}", err);
            std::process::exit(1);
        }
    };

    let meta = match read_meta(&root) {
        Ok(m) => m,
        Err(NsError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("error: no index found. Run 'ns index' to create one.");
            std::process::exit(1);
        }
        Err(NsError::Json(_)) => {
            eprintln!("error: corrupt index metadata. Run 'ns index' to rebuild.");
            std::process::exit(1);
        }
        Err(err) => {
            eprintln!("error: failed to read index: {}", err);
            std::process::exit(1);
        }
    };

    if meta.schema_version != SCHEMA_VERSION {
        eprintln!(
            "warning: index schema version {} does not match current version {}. Run 'ns index' to rebuild.",
            meta.schema_version, SCHEMA_VERSION
        );
    }

    println!("ns index status");
    println!("  schema version : {}", meta.schema_version);
    println!("  files indexed  : {}", meta.file_count);
    println!("  index size     : {}", format_bytes(meta.index_size_bytes));
    println!("  indexed at     : {}", meta.indexed_at);
    if let Some(commit) = &meta.git_commit {
        println!("  git commit     : {}", &commit[..commit.len().min(12)]);
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}
