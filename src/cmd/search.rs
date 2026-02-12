use std::path::PathBuf;

use crate::cmd::SearchArgs;
use crate::error::NsError;
use crate::searcher;
use crate::searcher::query::SearchOptions;
use crate::searcher::OutputMode;

pub fn run(args: &SearchArgs) {
    let root = match PathBuf::from(".").canonicalize() {
        Ok(p) => p,
        Err(err) => {
            eprintln!("error: cannot resolve current directory: {}", err);
            std::process::exit(1);
        }
    };

    let output_mode = if args.files_only {
        OutputMode::FilesOnly
    } else if args.json {
        OutputMode::Json
    } else {
        OutputMode::Text
    };

    let opts = SearchOptions {
        max_results: args.max_count,
        context_window: args.context,
        file_type: args.file_type.clone(),
        file_glob: args.file_glob.clone(),
        sym_only: args.sym,
        fuzzy: args.fuzzy,
    };

    match searcher::search(&root, &args.query, output_mode, &opts) {
        Ok((output, stats)) => {
            print!("{}", output);
            if stats.total_results == 0 {
                std::process::exit(1);
            }
        }
        Err(err) => {
            match &err {
                NsError::Io(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    eprintln!("error: no index found. Run `ns index` to create one.");
                }
                NsError::SchemaVersionMismatch { found, expected } => {
                    eprintln!(
                        "error: index schema version {} does not match expected {}. Run `ns index` to rebuild.",
                        found, expected
                    );
                }
                NsError::QueryParse(e) => {
                    eprintln!("error: invalid query: {}", e);
                }
                NsError::Glob(e) => {
                    eprintln!("error: invalid glob pattern: {}", e);
                }
                NsError::Json(_) => {
                    eprintln!("error: corrupt index metadata. Run `ns index` to rebuild.");
                }
                _ => {
                    eprintln!("error: search failed: {}", err);
                }
            }
            std::process::exit(1);
        }
    }
}
