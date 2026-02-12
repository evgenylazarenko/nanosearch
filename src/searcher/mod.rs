pub mod context;
pub mod format;
pub mod query;

use std::path::Path;

use crate::error::NsError;
use context::{extract_context, ContextLine};
use format::format_text;
use query::{execute_search, SearchResult, SearchStats};

/// A search result with extracted context lines, ready for display.
pub struct DisplayResult {
    pub rank: usize,
    pub result: SearchResult,
    pub context_lines: Vec<ContextLine>,
}

/// Runs the full search pipeline: query → context extraction → text formatting.
///
/// Returns the formatted output string and stats.
pub fn search(
    root: &Path,
    query_str: &str,
    max_results: usize,
    context_window: usize,
) -> Result<(String, SearchStats), NsError> {
    let (results, stats) = execute_search(root, query_str, max_results)?;

    let display_results: Vec<DisplayResult> = results
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
        .collect();

    let output = format_text(&display_results, &stats);
    Ok((output, stats))
}
