use std::path::PathBuf;

use crate::cmd::SearchArgs;
use crate::error::NsError;
use crate::searcher;
use crate::searcher::format::format_summary;
use crate::searcher::query::SearchOptions;
use crate::searcher::OutputMode;
use crate::indexer::writer::utc_timestamp_iso8601;
use crate::stats;

pub fn run(args: &SearchArgs) {
    let root = match PathBuf::from(".").canonicalize() {
        Ok(p) => p,
        Err(err) => {
            eprintln!("error: cannot resolve current directory: {}", err);
            std::process::exit(1);
        }
    };

    let is_json = args.json;
    let (output_mode, mode_str) = if args.files_only {
        (OutputMode::FilesOnly, "files")
    } else if args.json {
        (OutputMode::Json, "json")
    } else {
        (OutputMode::Text, "text")
    };

    let max_context_lines = if args.max_context_lines == 0 {
        Some(0) // 0 means unlimited
    } else {
        Some(args.max_context_lines)
    };

    // --budget 0 means unlimited (consistent with --max-context-lines 0)
    let budget = match args.budget {
        Some(0) => None,
        other => other,
    };

    let opts = SearchOptions {
        max_results: args.max_count,
        context_window: args.context,
        file_type: args.file_type.clone(),
        file_glob: args.file_glob.clone(),
        sym_only: args.sym,
        fuzzy: args.fuzzy,
        max_context_lines,
        budget,
    };

    match searcher::search(&root, &args.query, output_mode, &opts) {
        Ok(search_output) => {
            let output = &search_output.formatted;
            let stats = &search_output.stats;
            if stats.total_results == 0 {
                // JSON mode: print the body to stdout (structured data for consumers)
                if is_json {
                    print!("{}", output);
                }
                // Summary to stderr — consistent with exit 1 (rg convention)
                eprintln!("{}", format_summary(stats));
                std::process::exit(1);
            } else {
                print!("{}", output);
                if search_output.budget_exhausted {
                    eprintln!(
                        "warning: token budget exceeded, {} results omitted",
                        search_output.results_omitted
                    );
                }
                eprintln!("{}", format_summary(stats));
                stats::record_search(&root, output.len());
                stats::record_search_log(&root, stats::SearchLogEntry {
                    ts: utc_timestamp_iso8601(),
                    v: env!("CARGO_PKG_VERSION"),
                    query: args.query.clone(),
                    tokens: output.len() / 4,
                    lines: output.lines().count(),
                    files: stats.total_results,
                    mode: mode_str.to_string(),
                    budget,
                });
            }
        }
        Err(err) => {
            match &err {
                NsError::Io(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    eprintln!("error: no index found. Run 'ns index' to create one.");
                }
                NsError::SchemaVersionMismatch { .. } => {
                    eprintln!(
                        "error: index was built with an older version of ns. Run 'ns index' to rebuild."
                    );
                }
                NsError::QueryParse(e) => {
                    eprintln!("error: invalid query: {}", e);
                }
                NsError::Glob(e) => {
                    eprintln!("error: invalid glob pattern: {}", e);
                }
                NsError::Json(_) => {
                    eprintln!("error: corrupt index metadata. Run 'ns index' to rebuild.");
                }
                _ if err.is_lock_error() => {
                    eprintln!("error: index is locked by another process.");
                }
                _ => {
                    eprintln!("error: search failed: {}", err);
                }
            }
            std::process::exit(1);
        }
    }
}
