use super::DisplayResult;
use super::query::{SearchResult, SearchStats};

/// Formats search results as human-readable text output.
///
/// Format matches the concept doc:
/// ```text
///  [1] path/to/file.rs  (score: 12.4, lang: rust)
///      42: matching line content
///      43:     next line
/// ```
pub fn format_text(results: &[DisplayResult]) -> String {
    let mut out = String::new();

    for display in results {
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

        // Blank line between results
        out.push('\n');
    }

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

/// Formats search results as bare file paths, one per line.
///
/// Used with `-l`/`--files` flag. No scores, no context, no decoration.
pub fn format_files_only(results: &[SearchResult]) -> String {
    let mut out = String::new();
    for r in results {
        out.push_str(&r.path);
        out.push('\n');
    }
    out
}

/// Formats search results as a JSON object.
///
/// Structure:
/// ```json
/// {
///   "query": "EventStore",
///   "results": [
///     {
///       "rank": 1,
///       "path": "src/event_store.rs",
///       "score": 12.4,
///       "lang": "rust",
///       "matched_symbols": ["EventStore"],
///       "lines": [
///         { "num": 5, "text": "pub struct EventStore {" }
///       ]
///     }
///   ],
///   "stats": {
///     "total_results": 3,
///     "files_searched": 42,
///     "elapsed_ms": 7
///   }
/// }
/// ```
pub fn format_json(
    results: &[DisplayResult],
    stats: &SearchStats,
    query_str: &str,
) -> String {
    // Tokenize query for matched_symbols intersection
    let query_terms: Vec<String> = query_str
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_lowercase())
        .collect();

    let result_values: Vec<serde_json::Value> = results
        .iter()
        .map(|d| {
            // Case-insensitive intersection: symbols whose lowercase matches a query term
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

            serde_json::json!({
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
            })
        })
        .collect();

    let json = serde_json::json!({
        "query": query_str,
        "results": result_values,
        "stats": {
            "total_results": stats.total_results,
            "files_searched": stats.files_searched,
            "elapsed_ms": stats.elapsed_ms,
        },
    });

    serde_json::to_string_pretty(&json).unwrap_or_else(|_| "{}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::searcher::context::ContextLine;
    use crate::searcher::query::{SearchResult, SearchStats};
    use crate::searcher::DisplayResult;

    #[test]
    fn format_single_result() {
        let results = vec![DisplayResult {
            rank: 1,
            result: SearchResult {
                path: "src/main.rs".to_string(),
                score: 8.5,
                lang: Some("rust".to_string()),
                symbols_raw: vec!["main".to_string()],
                score_content: 6.0,
                score_symbols: 2.5,
                matched_fields: vec!["content".to_string(), "symbols".to_string()],
            },
            context_lines: vec![
                ContextLine {
                    line_number: 10,
                    text: "fn main() {".to_string(),
                },
                ContextLine {
                    line_number: 11,
                    text: "    println!(\"hello\");".to_string(),
                },
            ],
        }];

        let output = format_text(&results);
        assert!(output.contains("[1] src/main.rs"));
        assert!(output.contains("score: 8.5"));
        assert!(output.contains("lang: rust"));
        assert!(output.contains("  10: fn main()"));
        // Ranking annotation should appear
        assert!(output.contains("matched: content+symbols"), "should show matched fields annotation");
        // Summary is no longer part of format_text — see format_summary
        assert!(!output.contains("result (searched"), "summary should not be in format_text output");
    }

    #[test]
    fn format_no_results() {
        let results = vec![];
        let output = format_text(&results);
        assert!(output.is_empty(), "format_text with no results should return empty string");
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
    fn format_non_contiguous_lines_have_separator() {
        let results = vec![DisplayResult {
            rank: 1,
            result: SearchResult {
                path: "src/lib.rs".to_string(),
                score: 5.0,
                lang: Some("rust".to_string()),
                symbols_raw: vec![],
                score_content: 5.0,
                score_symbols: 0.0,
                matched_fields: vec!["content".to_string()],
            },
            context_lines: vec![
                ContextLine { line_number: 3, text: "use foo;".to_string() },
                ContextLine { line_number: 4, text: "use bar;".to_string() },
                // gap here (5-9 missing)
                ContextLine { line_number: 10, text: "fn foo() {}".to_string() },
            ],
        }];
        let output = format_text(&results);
        assert!(output.contains("..."), "should have separator between non-contiguous groups");
        // Separator should appear between line 4 and line 10, not before line 3
        let lines: Vec<&str> = output.lines().collect();
        let sep_idx = lines.iter().position(|l| l.contains("...")).unwrap();
        assert!(lines[sep_idx - 1].contains("4:"), "separator should follow line 4");
        assert!(lines[sep_idx + 1].contains("10:"), "separator should precede line 10");
    }

    #[test]
    fn format_contiguous_lines_no_separator() {
        let results = vec![DisplayResult {
            rank: 1,
            result: SearchResult {
                path: "src/lib.rs".to_string(),
                score: 5.0,
                lang: Some("rust".to_string()),
                symbols_raw: vec![],
                score_content: 5.0,
                score_symbols: 0.0,
                matched_fields: vec!["content".to_string()],
            },
            context_lines: vec![
                ContextLine { line_number: 1, text: "line1".to_string() },
                ContextLine { line_number: 2, text: "line2".to_string() },
                ContextLine { line_number: 3, text: "line3".to_string() },
            ],
        }];
        let output = format_text(&results);
        assert!(!output.contains("..."), "contiguous lines should have no separator");
    }

    #[test]
    fn format_unknown_lang() {
        let results = vec![DisplayResult {
            rank: 1,
            result: SearchResult {
                path: "README.md".to_string(),
                score: 2.0,
                lang: None,
                symbols_raw: vec![],
                score_content: 2.0,
                score_symbols: 0.0,
                matched_fields: vec!["content".to_string()],
            },
            context_lines: vec![],
        }];
        let output = format_text(&results);
        assert!(output.contains("lang: unknown"));
    }

    #[test]
    fn format_files_only_bare_paths() {
        let results = vec![
            SearchResult {
                path: "src/main.rs".to_string(),
                score: 8.5,
                lang: Some("rust".to_string()),
                symbols_raw: vec![],
                score_content: 8.5,
                score_symbols: 0.0,
                matched_fields: vec!["content".to_string()],
            },
            SearchResult {
                path: "src/lib.rs".to_string(),
                score: 5.0,
                lang: Some("rust".to_string()),
                symbols_raw: vec![],
                score_content: 5.0,
                score_symbols: 0.0,
                matched_fields: vec!["content".to_string()],
            },
        ];

        let output = format_files_only(&results);
        assert_eq!(output, "src/main.rs\nsrc/lib.rs\n");
    }

    #[test]
    fn format_json_structure() {
        let results = vec![DisplayResult {
            rank: 1,
            result: SearchResult {
                path: "src/event_store.rs".to_string(),
                score: 12.4,
                lang: Some("rust".to_string()),
                symbols_raw: vec!["EventStore".to_string(), "new".to_string()],
                score_content: 4.2,
                score_symbols: 2.8,
                matched_fields: vec!["content".to_string(), "symbols".to_string()],
            },
            context_lines: vec![
                ContextLine {
                    line_number: 5,
                    text: "pub struct EventStore {".to_string(),
                },
            ],
        }];
        let stats = SearchStats {
            total_results: 1,
            files_searched: 42,
            elapsed_ms: 7,
        };

        let output = format_json(&results, &stats, "EventStore");
        let parsed: serde_json::Value = serde_json::from_str(&output).expect("should be valid JSON");

        assert_eq!(parsed["query"], "EventStore");
        assert_eq!(parsed["results"][0]["rank"], 1);
        assert_eq!(parsed["results"][0]["path"], "src/event_store.rs");
        assert_eq!(parsed["results"][0]["lang"], "rust");
        assert_eq!(parsed["results"][0]["matched_symbols"][0], "EventStore");
        assert_eq!(parsed["results"][0]["lines"][0]["num"], 5);
        assert_eq!(parsed["stats"]["total_results"], 1);
        assert_eq!(parsed["stats"]["files_searched"], 42);

        // Feature 5: ranking_factors should be present
        let rf = &parsed["results"][0]["ranking_factors"];
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
    fn format_json_matched_symbols_case_insensitive() {
        let results = vec![DisplayResult {
            rank: 1,
            result: SearchResult {
                path: "src/foo.rs".to_string(),
                score: 5.0,
                lang: Some("rust".to_string()),
                symbols_raw: vec!["EventStore".to_string(), "unrelated_fn".to_string()],
                score_content: 3.0,
                score_symbols: 2.0,
                matched_fields: vec!["content".to_string(), "symbols".to_string()],
            },
            context_lines: vec![],
        }];
        let stats = SearchStats { total_results: 1, files_searched: 1, elapsed_ms: 0 };

        // Query "eventstore" (lowercase) should match "EventStore" (original case)
        let output = format_json(&results, &stats, "eventstore");
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        let matched = parsed["results"][0]["matched_symbols"].as_array().unwrap();
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0], "EventStore");
    }
}
