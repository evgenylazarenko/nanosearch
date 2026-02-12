use super::context::ContextLine;
use super::query::{SearchResult, SearchStats};

/// A search result with extracted context lines, ready for display.
pub struct DisplayResult {
    pub rank: usize,
    pub result: SearchResult,
    pub context_lines: Vec<ContextLine>,
}

/// Formats search results as human-readable text output.
///
/// Format matches the concept doc:
/// ```text
///  [1] path/to/file.rs  (score: 12.4, lang: rust)
///      42: matching line content
///      43:     next line
/// ```
pub fn format_text(results: &[DisplayResult], stats: &SearchStats) -> String {
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

    // Summary line (correct pluralization)
    let result_word = if stats.total_results == 1 { "result" } else { "results" };
    let file_word = if stats.files_searched == 1 { "file" } else { "files" };
    out.push_str(&format!(
        "{} {} (searched {} {} in {}ms)\n",
        stats.total_results, result_word, stats.files_searched, file_word, stats.elapsed_ms
    ));

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::searcher::context::ContextLine;
    use crate::searcher::query::{SearchResult, SearchStats};

    #[test]
    fn format_single_result() {
        let results = vec![DisplayResult {
            rank: 1,
            result: SearchResult {
                path: "src/main.rs".to_string(),
                score: 8.5,
                lang: Some("rust".to_string()),
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
        let stats = SearchStats {
            total_results: 1,
            files_searched: 42,
            elapsed_ms: 3,
        };

        let output = format_text(&results, &stats);
        assert!(output.contains("[1] src/main.rs"));
        assert!(output.contains("score: 8.5"));
        assert!(output.contains("lang: rust"));
        assert!(output.contains("  10: fn main()"));
        assert!(output.contains("1 result (searched 42 files in 3ms)"));
    }

    #[test]
    fn format_no_results() {
        let results = vec![];
        let stats = SearchStats {
            total_results: 0,
            files_searched: 100,
            elapsed_ms: 1,
        };

        let output = format_text(&results, &stats);
        assert!(output.contains("0 results (searched 100 files"));
    }

    #[test]
    fn format_non_contiguous_lines_have_separator() {
        let results = vec![DisplayResult {
            rank: 1,
            result: SearchResult {
                path: "src/lib.rs".to_string(),
                score: 5.0,
                lang: Some("rust".to_string()),
            },
            context_lines: vec![
                ContextLine { line_number: 3, text: "use foo;".to_string() },
                ContextLine { line_number: 4, text: "use bar;".to_string() },
                // gap here (5-9 missing)
                ContextLine { line_number: 10, text: "fn foo() {}".to_string() },
            ],
        }];
        let stats = SearchStats { total_results: 1, files_searched: 1, elapsed_ms: 0 };

        let output = format_text(&results, &stats);
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
            },
            context_lines: vec![
                ContextLine { line_number: 1, text: "line1".to_string() },
                ContextLine { line_number: 2, text: "line2".to_string() },
                ContextLine { line_number: 3, text: "line3".to_string() },
            ],
        }];
        let stats = SearchStats { total_results: 1, files_searched: 1, elapsed_ms: 0 };

        let output = format_text(&results, &stats);
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
            },
            context_lines: vec![],
        }];
        let stats = SearchStats {
            total_results: 1,
            files_searched: 10,
            elapsed_ms: 0,
        };

        let output = format_text(&results, &stats);
        assert!(output.contains("lang: unknown"));
    }
}
