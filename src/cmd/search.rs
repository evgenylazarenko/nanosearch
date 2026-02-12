use std::path::PathBuf;

use crate::cmd::SearchArgs;
use crate::error::NsError;
use crate::searcher;

pub fn run(args: &SearchArgs) {
    let root = match PathBuf::from(".").canonicalize() {
        Ok(p) => p,
        Err(err) => {
            eprintln!("error: cannot resolve current directory: {}", err);
            std::process::exit(1);
        }
    };

    match searcher::search(&root, &args.query, args.max_count, args.context) {
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
