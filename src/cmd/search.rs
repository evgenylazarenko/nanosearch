use std::path::PathBuf;

use crate::cmd::SearchArgs;
use crate::searcher;

pub fn run(args: &SearchArgs) {
    let root = match PathBuf::from(".").canonicalize() {
        Ok(p) => p,
        Err(err) => {
            eprintln!("error: cannot resolve current directory: {}", err);
            std::process::exit(1);
        }
    };

    // Quick existence check for actionable error message.
    // The full meta.json read + schema validation happens inside open_index,
    // called by execute_search — we don't duplicate it here.
    if !root.join(".ns").join("meta.json").exists() {
        eprintln!("error: no index found. Run 'ns index' to create one.");
        std::process::exit(1);
    }

    match searcher::search(&root, &args.query, args.max_count, args.context) {
        Ok((output, stats)) => {
            print!("{}", output);
            if stats.total_results == 0 {
                std::process::exit(1);
            }
        }
        Err(err) => {
            eprintln!("error: search failed: {}", err);
            std::process::exit(1);
        }
    }
}
