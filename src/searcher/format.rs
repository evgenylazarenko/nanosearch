use super::DisplayResult;
use super::query::SearchStats;

/// Formats a single DisplayResult as human-readable text.
///
/// Used by the incremental budget-aware pipeline.
pub fn format_single_text(display: &DisplayResult) -> String {
    let mut out = String::new();

    // Header line: [rank] path (score, lang)
    let lang_str = display
        .result
        .lang
        .as_deref()
        .unwrap_or("unknown");

    out.push_str(&format!(
        " [{}] {}  (score: {:.1}, lang: {})\n",
        display.rank, display.result.path, display.result.score, lang_str
    ));

    // Short ranking annotation when there are matched fields
    if !display.result.matched_fields.is_empty() {
        let fields = display.result.matched_fields.join("+");
        out.push_str(&format!(
            "      ~ matched: {}, bm25_content: {:.1}, bm25_symbols: {:.1}\n",
            fields, display.result.score_content, display.result.score_symbols
        ));
    }

    // Context lines — insert "..." separator between non-contiguous groups
    let mut prev_line_number: Option<usize> = None;
    for line in &display.context_lines {
        if let Some(prev) = prev_line_number {
            if line.line_number > prev + 1 {
                out.push_str("          ...\n");
            }
        }
        out.push_str(&format!(
            "     {:>4}: {}\n",
            line.line_number, line.text
        ));
        prev_line_number = Some(line.line_number);
    }

    // Truncation indicator
    if display.truncated_count > 0 {
        out.push_str(&format!(
            "      ... ({} more matching lines)\n",
            display.truncated_count
        ));
    }

    // Blank line between results
    out.push('\n');

    out
}

/// Formats the search summary line (e.g. "3 results (searched 42 files in 2ms)").
///
/// Separated from `format_text` so the CLI layer can direct this to stderr,
/// keeping stdout reserved for result data only.
pub fn format_summary(stats: &SearchStats) -> String {
    let result_word = if stats.total_results == 1 { "result" } else { "results" };
    let file_word = if stats.files_searched == 1 { "file" } else { "files" };
    format!(
        "{} {} (searched {} {} in {}ms)",
        stats.total_results, result_word, stats.files_searched, file_word, stats.elapsed_ms
    )
}

/// Formats a single DisplayResult as a JSON value.
///
/// Used by the incremental budget-aware pipeline.
pub fn format_single_json_value(d: &DisplayResult, query_str: &str) -> serde_json::Value {
    let query_terms: Vec<String> = query_str
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_lowercase())
        .collect();

    let matched: Vec<&str> = d
        .result
        .symbols_raw
        .iter()
        .filter(|sym| {
            let lower = sym.to_lowercase();
            query_terms.iter().any(|qt| lower.contains(qt))
        })
        .map(|s| s.as_str())
        .collect();

    let lines: Vec<serde_json::Value> = d
        .context_lines
        .iter()
        .map(|cl| {
            serde_json::json!({
                "num": cl.line_number,
                "text": cl.text,
            })
        })
        .collect();

    let mut value = serde_json::json!({
        "rank": d.rank,
        "path": d.result.path,
        "score": d.result.score,
        "lang": d.result.lang,
        "matched_symbols": matched,
        "lines": lines,
        "ranking_factors": {
            "bm25_content": ((d.result.score_content as f64) * 10.0).round() / 10.0,
            "bm25_symbols": ((d.result.score_symbols as f64) * 10.0).round() / 10.0,
            "symbol_boost": "3x",
            "matched_fields": d.result.matched_fields,
        },
    });

    if d.truncated_count > 0 {
        value["truncated_lines"] = serde_json::json!(d.truncated_count);
    }

    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::searcher::context::ContextLine;
    use crate::searcher::query::{SearchResult, SearchStats};
    use crate::searcher::DisplayResult;

    /// Helper: build a DisplayResult for testing.
    fn make_display(
        rank: usize,
        path: &str,
        score: f32,
        lang: Option<&str>,
        symbols_raw: Vec<&str>,
        score_content: f32,
        score_symbols: f32,
        matched_fields: Vec<&str>,
        context_lines: Vec<ContextLine>,
        truncated_count: usize,
    ) -> DisplayResult {
        DisplayResult {
            rank,
            result: SearchResult {
                path: path.to_string(),
                score,
                lang: lang.map(|s| s.to_string()),
                symbols_raw: symbols_raw.into_iter().map(|s| s.to_string()).collect(),
                score_content,
                score_symbols,
                matched_fields: matched_fields.into_iter().map(|s| s.to_string()).collect(),
            },
            context_lines,
            truncated_count,
        }
    }

    #[test]
    fn single_text_formats_result_correctly() {
        let display = make_display(
            1, "src/main.rs", 8.5, Some("rust"),
            vec!["main"], 6.0, 2.5,
            vec!["content", "symbols"],
            vec![
                ContextLine { line_number: 10, text: "fn main() {".to_string() },
                ContextLine { line_number: 11, text: "    println!(\"hello\");".to_string() },
            ],
            0,
        );

        let output = format_single_text(&display);
        assert!(output.contains("[1] src/main.rs"));
        assert!(output.contains("score: 8.5"));
        assert!(output.contains("lang: rust"));
        assert!(output.contains("  10: fn main()"));
        assert!(output.contains("matched: content+symbols"), "should show matched fields annotation");
        assert!(!output.contains("result (searched"), "summary should not be in format output");
    }

    #[test]
    fn format_summary_correct() {
        let stats = SearchStats {
            total_results: 3,
            files_searched: 42,
            elapsed_ms: 2,
        };
        assert_eq!(format_summary(&stats), "3 results (searched 42 files in 2ms)");

        let stats_one = SearchStats {
            total_results: 1,
            files_searched: 1,
            elapsed_ms: 0,
        };
        assert_eq!(format_summary(&stats_one), "1 result (searched 1 file in 0ms)");

        let stats_zero = SearchStats {
            total_results: 0,
            files_searched: 100,
            elapsed_ms: 1,
        };
        assert_eq!(format_summary(&stats_zero), "0 results (searched 100 files in 1ms)");
    }

    #[test]
    fn single_text_non_contiguous_lines_have_separator() {
        let display = make_display(
            1, "src/lib.rs", 5.0, Some("rust"),
            vec![], 5.0, 0.0,
            vec!["content"],
            vec![
                ContextLine { line_number: 3, text: "use foo;".to_string() },
                ContextLine { line_number: 4, text: "use bar;".to_string() },
                // gap here (5-9 missing)
                ContextLine { line_number: 10, text: "fn foo() {}".to_string() },
            ],
            0,
        );
        let output = format_single_text(&display);
        assert!(output.contains("..."), "should have separator between non-contiguous groups");
        let lines: Vec<&str> = output.lines().collect();
        let sep_idx = lines.iter().position(|l| l.contains("...")).unwrap();
        assert!(lines[sep_idx - 1].contains("4:"), "separator should follow line 4");
        assert!(lines[sep_idx + 1].contains("10:"), "separator should precede line 10");
    }

    #[test]
    fn single_text_contiguous_lines_no_separator() {
        let display = make_display(
            1, "src/lib.rs", 5.0, Some("rust"),
            vec![], 5.0, 0.0,
            vec!["content"],
            vec![
                ContextLine { line_number: 1, text: "line1".to_string() },
                ContextLine { line_number: 2, text: "line2".to_string() },
                ContextLine { line_number: 3, text: "line3".to_string() },
            ],
            0,
        );
        let output = format_single_text(&display);
        assert!(!output.contains("..."), "contiguous lines should have no separator");
    }

    #[test]
    fn single_text_unknown_lang() {
        let display = make_display(
            1, "README.md", 2.0, None,
            vec![], 2.0, 0.0,
            vec!["content"],
            vec![],
            0,
        );
        let output = format_single_text(&display);
        assert!(output.contains("lang: unknown"));
    }

    #[test]
    fn single_json_value_structure() {
        let display = make_display(
            1, "src/event_store.rs", 12.4, Some("rust"),
            vec!["EventStore", "new"], 4.2, 2.8,
            vec!["content", "symbols"],
            vec![
                ContextLine { line_number: 5, text: "pub struct EventStore {".to_string() },
            ],
            0,
        );

        let parsed = format_single_json_value(&display, "EventStore");

        assert_eq!(parsed["rank"], 1);
        assert_eq!(parsed["path"], "src/event_store.rs");
        assert_eq!(parsed["lang"], "rust");
        assert_eq!(parsed["matched_symbols"][0], "EventStore");
        assert_eq!(parsed["lines"][0]["num"], 5);

        // ranking_factors should be present
        let rf = &parsed["ranking_factors"];
        assert!(rf.is_object(), "ranking_factors should be an object");
        assert_eq!(rf["bm25_content"], 4.2);
        assert_eq!(rf["bm25_symbols"], 2.8);
        assert_eq!(rf["symbol_boost"], "3x");
        let mf = rf["matched_fields"].as_array().unwrap();
        assert_eq!(mf.len(), 2);
        assert_eq!(mf[0], "content");
        assert_eq!(mf[1], "symbols");
    }

    #[test]
    fn single_json_value_matched_symbols_case_insensitive() {
        let display = make_display(
            1, "src/foo.rs", 5.0, Some("rust"),
            vec!["EventStore", "unrelated_fn"], 3.0, 2.0,
            vec!["content", "symbols"],
            vec![],
            0,
        );

        // Query "eventstore" (lowercase) should match "EventStore" (original case)
        let parsed = format_single_json_value(&display, "eventstore");
        let matched = parsed["matched_symbols"].as_array().unwrap();
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0], "EventStore");
    }

    #[test]
    fn single_text_shows_truncation_indicator() {
        let display = make_display(
            1, "src/big.rs", 5.0, Some("rust"),
            vec![], 5.0, 0.0,
            vec!["content"],
            vec![
                ContextLine { line_number: 1, text: "line one".to_string() },
                ContextLine { line_number: 2, text: "line two".to_string() },
            ],
            47,
        );
        let output = format_single_text(&display);
        assert!(
            output.contains("... (47 more matching lines)"),
            "should show truncation indicator, got:\n{}",
            output
        );
    }

    #[test]
    fn single_text_no_truncation_indicator_when_zero() {
        let display = make_display(
            1, "src/small.rs", 5.0, Some("rust"),
            vec![], 5.0, 0.0,
            vec!["content"],
            vec![
                ContextLine { line_number: 1, text: "line one".to_string() },
            ],
            0,
        );
        let output = format_single_text(&display);
        assert!(
            !output.contains("more matching lines"),
            "should not show truncation indicator when truncated_count=0"
        );
    }

    #[test]
    fn single_json_value_shows_truncated_lines() {
        let display = make_display(
            1, "src/big.rs", 5.0, Some("rust"),
            vec![], 5.0, 0.0,
            vec!["content"],
            vec![
                ContextLine { line_number: 1, text: "line one".to_string() },
            ],
            47,
        );

        let parsed = format_single_json_value(&display, "big");
        assert_eq!(
            parsed["truncated_lines"], 47,
            "JSON should include truncated_lines field"
        );
    }

    #[test]
    fn single_json_value_no_truncated_lines_when_zero() {
        let display = make_display(
            1, "src/small.rs", 5.0, Some("rust"),
            vec![], 5.0, 0.0,
            vec!["content"],
            vec![],
            0,
        );

        let parsed = format_single_json_value(&display, "small");
        assert!(
            parsed["truncated_lines"].is_null(),
            "JSON should not include truncated_lines when truncated_count=0"
        );
    }
}
