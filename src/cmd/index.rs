use std::path::PathBuf;

use crate::cmd::IndexArgs;
use crate::indexer;

pub fn run(args: &IndexArgs) {
    if args.incremental {
        eprintln!("error: incremental indexing not yet implemented. Run `ns index` without --incremental.");
        std::process::exit(1);
    }

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

    match indexer::run_full_index(&root, args.max_file_size) {
        Ok(_) => {}
        Err(err) => {
            eprintln!("error: indexing failed: {}", err);
            std::process::exit(1);
        }
    }
}
