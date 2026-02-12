pub mod context;
pub mod format;
pub mod query;

use std::path::Path;

use crate::error::NsError;
use context::{extract_context, ContextLine};
use format::{format_files_only, format_json, format_text};
use query::{execute_search, SearchOptions, SearchResult, SearchStats};

/// A search result with extracted context lines, ready for display.
#[derive(Debug)]
pub struct DisplayResult {
    pub rank: usize,
    pub result: SearchResult,
    pub context_lines: Vec<ContextLine>,
}

/// Output mode for formatting results.
pub enum OutputMode {
    /// Human-readable text (default).
    Text,
    /// Bare file paths, one per line (`-l`/`--files`).
    FilesOnly,
    /// Machine-readable JSON (`--json`).
    Json,
}

/// Runs the full search pipeline: query → context extraction → formatting.
///
/// Returns the formatted output string and stats.
pub fn search(
    root: &Path,
    query_str: &str,
    output_mode: OutputMode,
    opts: &SearchOptions,
) -> Result<(String, SearchStats), NsError> {
    let (results, stats) = execute_search(root, query_str, opts)?;

    match output_mode {
        OutputMode::FilesOnly => {
            let output = format_files_only(&results);
            Ok((output, stats))
        }
        OutputMode::Json => {
            let display_results =
                build_display_results(root, results, query_str, opts.context_window);
            let output = format_json(&display_results, &stats, query_str);
            Ok((output, stats))
        }
        OutputMode::Text => {
            let display_results =
                build_display_results(root, results, query_str, opts.context_window);
            let output = format_text(&display_results);
            Ok((output, stats))
        }
    }
}

fn build_display_results(
    root: &Path,
    results: Vec<SearchResult>,
    query_str: &str,
    context_window: usize,
) -> Vec<DisplayResult> {
    results
        .into_iter()
        .enumerate()
        .map(|(i, result)| {
            let context_lines =
                extract_context(root, &result.path, query_str, context_window);
            DisplayResult {
                rank: i + 1,
                result,
                context_lines,
            }
        })
        .collect()
}
