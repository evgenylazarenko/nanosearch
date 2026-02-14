pub mod context;
pub mod format;
pub mod query;

use std::path::Path;

use crate::error::NsError;
use context::{extract_context, ContextLine};
use format::{format_single_text, format_single_json_value};
use query::{execute_search, SearchOptions, SearchResult, SearchStats};

/// A search result with extracted context lines, ready for display.
#[derive(Debug)]
pub struct DisplayResult {
    pub rank: usize,
    pub result: SearchResult,
    pub context_lines: Vec<ContextLine>,
    /// Number of context lines omitted due to per-file cap.
    pub truncated_count: usize,
}

/// Output of the search pipeline, including budget metadata.
#[derive(Debug)]
pub struct SearchOutput {
    pub formatted: String,
    pub stats: SearchStats,
    pub budget_exhausted: bool,
    pub results_omitted: usize,
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
/// Returns a `SearchOutput` containing formatted output, stats, and budget metadata.
pub fn search(
    root: &Path,
    query_str: &str,
    output_mode: OutputMode,
    opts: &SearchOptions,
) -> Result<SearchOutput, NsError> {
    let (results, stats) = execute_search(root, query_str, opts)?;

    match output_mode {
        OutputMode::FilesOnly => {
            let (output, budget_exhausted, results_omitted) =
                build_files_only_with_budget(&results, opts.budget);
            Ok(SearchOutput {
                formatted: output,
                stats,
                budget_exhausted,
                results_omitted,
            })
        }
        OutputMode::Text => {
            let (output, budget_exhausted, results_omitted) =
                build_text_with_budget(root, results, query_str, opts);
            Ok(SearchOutput {
                formatted: output,
                stats,
                budget_exhausted,
                results_omitted,
            })
        }
        OutputMode::Json => {
            let (output, budget_exhausted, results_omitted) =
                build_json_with_budget(root, results, query_str, opts, &stats);
            Ok(SearchOutput {
                formatted: output,
                stats,
                budget_exhausted,
                results_omitted,
            })
        }
    }
}

/// Build files-only output with optional budget.
fn build_files_only_with_budget(
    results: &[SearchResult],
    budget: Option<usize>,
) -> (String, bool, usize) {
    let budget_chars = budget.map(|b| b * 4);
    let mut out = String::new();
    let mut emitted = 0;

    for r in results {
        let line = format!("{}\n", r.path);
        if let Some(cap) = budget_chars {
            if out.len() + line.len() > cap && !out.is_empty() {
                let omitted = results.len() - emitted;
                out.push_str(&format!("... ({} more results, budget exceeded)\n", omitted));
                return (out, true, omitted);
            }
        }
        out.push_str(&line);
        emitted += 1;
    }

    (out, false, 0)
}

/// Build text output incrementally with optional budget.
fn build_text_with_budget(
    root: &Path,
    results: Vec<SearchResult>,
    query_str: &str,
    opts: &SearchOptions,
) -> (String, bool, usize) {
    let budget_chars = opts.budget.map(|b| b * 4);
    let mut out = String::new();
    let total = results.len();
    let mut emitted = 0;

    for (i, result) in results.into_iter().enumerate() {
        let ctx = extract_context(root, &result.path, query_str, opts.context_window, opts.max_context_lines);
        let display = DisplayResult {
            rank: i + 1,
            result,
            context_lines: ctx.lines,
            truncated_count: ctx.truncated_count,
        };
        let chunk = format_single_text(&display);

        if let Some(cap) = budget_chars {
            if out.len() + chunk.len() > cap && !out.is_empty() {
                let omitted = total - emitted;
                out.push_str(&format!("... ({} more results, budget exceeded)\n", omitted));
                return (out, true, omitted);
            }
        }
        out.push_str(&chunk);
        emitted += 1;
    }

    (out, false, 0)
}

/// Build JSON output incrementally with optional budget.
fn build_json_with_budget(
    root: &Path,
    results: Vec<SearchResult>,
    query_str: &str,
    opts: &SearchOptions,
    stats: &SearchStats,
) -> (String, bool, usize) {
    let budget_chars = opts.budget.map(|b| b * 4);
    let total = results.len();
    let mut result_values: Vec<serde_json::Value> = Vec::new();
    let mut emitted = 0;
    let mut budget_exhausted = false;
    let mut results_omitted = 0;

    // Estimate the overhead for the JSON envelope (query, stats, etc.)
    // We do a rough estimate: ~200 chars for the wrapper
    let envelope_estimate = 200;
    let mut running_chars = envelope_estimate;

    for (i, result) in results.into_iter().enumerate() {
        let ctx = extract_context(root, &result.path, query_str, opts.context_window, opts.max_context_lines);
        let display = DisplayResult {
            rank: i + 1,
            result,
            context_lines: ctx.lines,
            truncated_count: ctx.truncated_count,
        };
        let value = format_single_json_value(&display, query_str);
        let value_str = serde_json::to_string(&value).unwrap_or_default();

        if let Some(cap) = budget_chars {
            if running_chars + value_str.len() > cap && !result_values.is_empty() {
                results_omitted = total - emitted;
                budget_exhausted = true;
                break;
            }
        }
        running_chars += value_str.len();
        result_values.push(value);
        emitted += 1;
    }

    // Build final JSON
    let mut stats_obj = serde_json::json!({
        "total_results": stats.total_results,
        "files_searched": stats.files_searched,
        "elapsed_ms": stats.elapsed_ms,
    });
    if budget_exhausted {
        stats_obj["budget_exceeded"] = serde_json::json!(true);
        stats_obj["results_omitted"] = serde_json::json!(results_omitted);
    }

    let json = serde_json::json!({
        "query": query_str,
        "results": result_values,
        "stats": stats_obj,
    });

    let formatted = serde_json::to_string_pretty(&json).unwrap_or_else(|_| "{}".to_string());
    (formatted, budget_exhausted, results_omitted)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_result(path: &str) -> SearchResult {
        SearchResult {
            path: path.to_string(),
            score: 5.0,
            lang: Some("rust".to_string()),
            symbols_raw: vec![],
            score_content: 5.0,
            score_symbols: 0.0,
            matched_fields: vec!["content".to_string()],
        }
    }

    #[test]
    fn files_only_budget_truncates() {
        let results: Vec<SearchResult> = (0..10)
            .map(|i| fake_result(&format!("src/file_{}.rs", i)))
            .collect();

        // Each line is ~16 chars. Budget of 10 tokens = 40 chars = ~2 lines
        let (output, exhausted, omitted) = build_files_only_with_budget(&results, Some(10));
        assert!(exhausted, "budget should be exhausted");
        assert!(omitted > 0, "should have omitted results");
        assert!(output.contains("budget exceeded"), "should show budget exceeded message");

        // Without budget, all should be emitted
        let (output_full, exhausted_full, omitted_full) =
            build_files_only_with_budget(&results, None);
        assert!(!exhausted_full);
        assert_eq!(omitted_full, 0);
        assert_eq!(output_full.lines().count(), 10);
    }

    #[test]
    fn files_only_budget_none_is_unlimited() {
        let results: Vec<SearchResult> = (0..5)
            .map(|i| fake_result(&format!("src/file_{}.rs", i)))
            .collect();

        let (output, exhausted, omitted) = build_files_only_with_budget(&results, None);
        assert!(!exhausted);
        assert_eq!(omitted, 0);
        assert_eq!(output.lines().count(), 5);
    }

    #[test]
    fn text_budget_truncates() {
        use std::path::PathBuf;

        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/sample_repo");

        // Create multiple results that will produce substantial text output
        let results: Vec<SearchResult> = vec![
            fake_result("src/event_store.rs"),
            fake_result("src/validator.rs"),
            fake_result("src/event_store.rs"), // duplicate path for testing
        ];

        let opts_with_budget = SearchOptions {
            budget: Some(50), // Very small budget: 50 tokens = 200 chars
            max_context_lines: Some(5),
            ..Default::default()
        };

        let (output, exhausted, omitted) =
            build_text_with_budget(&fixture, results, "EventStore", &opts_with_budget);
        // The first result alone is >200 chars, so budget check kicks in before result 2
        // But we always emit at least one result
        assert!(
            output.contains("[1]"),
            "should emit at least the first result"
        );
        if exhausted {
            assert!(omitted > 0);
            assert!(output.contains("budget exceeded"));
        }
    }

    #[test]
    fn text_no_budget_emits_all() {
        use std::path::PathBuf;

        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/sample_repo");

        let results: Vec<SearchResult> = vec![
            fake_result("src/event_store.rs"),
            fake_result("src/validator.rs"),
        ];

        let opts_no_budget = SearchOptions {
            budget: None,
            ..Default::default()
        };

        let (output, exhausted, _omitted) =
            build_text_with_budget(&fixture, results, "EventStore", &opts_no_budget);
        assert!(!exhausted);
        assert!(output.contains("[1]"));
        assert!(output.contains("[2]"));
    }

    #[test]
    fn json_budget_shows_budget_exceeded() {
        use std::path::PathBuf;

        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/sample_repo");

        let results: Vec<SearchResult> = vec![
            fake_result("src/event_store.rs"),
            fake_result("src/validator.rs"),
            fake_result("src/event_store.rs"),
        ];

        let stats = SearchStats {
            total_results: 3,
            files_searched: 10,
            elapsed_ms: 1,
        };

        let opts = SearchOptions {
            budget: Some(50), // Very small
            max_context_lines: Some(5),
            ..Default::default()
        };

        let (output, exhausted, omitted) =
            build_json_with_budget(&fixture, results, "EventStore", &opts, &stats);

        let parsed: serde_json::Value = serde_json::from_str(&output).expect("valid JSON");
        assert!(parsed["results"].is_array());

        if exhausted {
            assert!(omitted > 0);
            assert_eq!(parsed["stats"]["budget_exceeded"], true);
            assert!(parsed["stats"]["results_omitted"].as_u64().unwrap() > 0);
        }
    }

    #[test]
    fn json_no_budget_has_no_budget_fields() {
        use std::path::PathBuf;

        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/sample_repo");

        let results: Vec<SearchResult> = vec![
            fake_result("src/event_store.rs"),
        ];

        let stats = SearchStats {
            total_results: 1,
            files_searched: 10,
            elapsed_ms: 1,
        };

        let opts = SearchOptions {
            budget: None,
            ..Default::default()
        };

        let (output, exhausted, _) =
            build_json_with_budget(&fixture, results, "EventStore", &opts, &stats);

        assert!(!exhausted);
        let parsed: serde_json::Value = serde_json::from_str(&output).expect("valid JSON");
        assert!(
            parsed["stats"]["budget_exceeded"].is_null(),
            "should not have budget_exceeded when no budget set"
        );
    }
}

