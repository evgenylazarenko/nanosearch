use std::collections::BTreeSet;
use std::path::Path;

/// A single line from a matched file, with its 1-based line number.
#[derive(Debug)]
pub struct ContextLine {
    pub line_number: usize,
    pub text: String,
}

/// Result of context extraction, including truncation info.
#[derive(Debug)]
pub struct ContextResult {
    pub lines: Vec<ContextLine>,
    /// Number of additional matching lines that were omitted due to the cap.
    /// 0 when no truncation occurred.
    pub truncated_count: usize,
}

/// Extracts context lines from a file that matched a search query.
///
/// For each query term, finds all lines containing it (case-insensitive),
/// then expands each match by ±`context_window` lines. Overlapping ranges
/// are merged. Returns lines sorted by line number.
///
/// When `max_lines` is `Some(n)`, at most `n` context lines are returned.
/// If the total would exceed the cap, the result is truncated to the first
/// `n` lines and `truncated_count` records how many were omitted.
/// `max_lines` of `Some(0)` means unlimited (no cap).
///
/// If the file cannot be read (deleted/moved since indexing), returns an empty result.
pub fn extract_context(
    root: &Path,
    rel_path: &str,
    query: &str,
    context_window: usize,
    max_lines: Option<usize>,
) -> ContextResult {
    let empty = ContextResult {
        lines: Vec::new(),
        truncated_count: 0,
    };

    let full_path = root.join(rel_path);
    let content = match std::fs::read_to_string(&full_path) {
        Ok(s) => s,
        Err(_) => return empty,
    };

    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();
    if total_lines == 0 {
        return empty;
    }

    // Tokenize the query by splitting on non-alphanumeric boundaries, then lowercase.
    // This mirrors tantivy's default tokenizer behavior — e.g. "EventStore.new" becomes
    // ["eventstore", "new"], "HashMap<String>" becomes ["hashmap", "string"].
    let terms: Vec<String> = tokenize_query(query);

    if terms.is_empty() {
        return empty;
    }

    // Find all line indices (0-based) that contain at least one query term
    let mut match_indices = BTreeSet::new();
    for (i, line) in lines.iter().enumerate() {
        let lower = line.to_lowercase();
        for term in &terms {
            if lower.contains(term.as_str()) {
                match_indices.insert(i);
                break;
            }
        }
    }

    if match_indices.is_empty() {
        return empty;
    }

    // Expand matches by ±context_window, collecting all line indices to include
    let mut include_indices = BTreeSet::new();
    for &idx in &match_indices {
        let start = idx.saturating_sub(context_window);
        let end = (idx + context_window).min(total_lines - 1);
        for i in start..=end {
            include_indices.insert(i);
        }
    }

    // Apply per-file context line cap
    // max_lines of Some(0) means unlimited (same as None)
    let cap = match max_lines {
        Some(0) | None => usize::MAX,
        Some(n) => n,
    };
    let total_context = include_indices.len();
    let truncated_count = if total_context > cap {
        total_context - cap
    } else {
        0
    };

    // Build context lines (1-based line numbers), taking at most `cap`
    let context_lines: Vec<ContextLine> = include_indices
        .iter()
        .take(cap)
        .map(|&i| ContextLine {
            line_number: i + 1,
            text: lines[i].to_string(),
        })
        .collect();

    ContextResult {
        lines: context_lines,
        truncated_count,
    }
}

/// Tokenizes a query string the same way tantivy's default tokenizer does:
/// split on non-alphanumeric boundaries, lowercase each token, drop empties.
fn tokenize_query(query: &str) -> Vec<String> {
    query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn extracts_matching_lines_with_context() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/sample_repo");

        // "EventStore" appears in event_store.rs
        let result = extract_context(&fixture, "src/event_store.rs", "EventStore", 1, None);

        assert!(!result.lines.is_empty(), "should find lines matching EventStore");
        assert_eq!(result.truncated_count, 0, "should not be truncated with no cap");

        // Verify we got the struct definition line
        let has_struct = result.lines.iter().any(|l| l.text.contains("pub struct EventStore"));
        assert!(has_struct, "should find 'pub struct EventStore' line");

        // Verify line numbers are in order
        for window in result.lines.windows(2) {
            assert!(
                window[0].line_number < window[1].line_number,
                "lines should be in order"
            );
        }
    }

    #[test]
    fn context_window_zero_returns_exact_matches() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/sample_repo");

        let result = extract_context(&fixture, "src/event_store.rs", "EventStore", 0, None);

        // Every returned line should contain "EventStore" (case-insensitive)
        for line in &result.lines {
            assert!(
                line.text.to_lowercase().contains("eventstore"),
                "with context=0, line {} '{}' should contain the term",
                line.line_number,
                line.text
            );
        }
    }

    #[test]
    fn missing_file_returns_empty() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/sample_repo");

        let result = extract_context(&fixture, "nonexistent.rs", "anything", 1, None);
        assert!(result.lines.is_empty(), "missing file should return empty vec");
        assert_eq!(result.truncated_count, 0);
    }

    #[test]
    fn tokenize_splits_on_punctuation() {
        let terms = tokenize_query("EventStore.new");
        assert_eq!(terms, vec!["eventstore", "new"]);

        let terms = tokenize_query("HashMap<String>");
        assert_eq!(terms, vec!["hashmap", "string"]);

        let terms = tokenize_query("foo::bar_baz");
        assert_eq!(terms, vec!["foo", "bar", "baz"]);

        let terms = tokenize_query("  spaced   out  ");
        assert_eq!(terms, vec!["spaced", "out"]);
    }

    #[test]
    fn multiterm_query_matches() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/sample_repo");

        // "validate port" — both terms appear in validator.rs
        let result = extract_context(&fixture, "src/validator.rs", "validate port", 0, None);

        // Should find lines containing either "validate" or "port"
        assert!(!result.lines.is_empty(), "should find lines for multi-term query");
    }

    #[test]
    fn max_lines_caps_context() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/sample_repo");

        // First, get all lines without cap to know how many there are
        let full = extract_context(&fixture, "src/event_store.rs", "EventStore", 1, None);
        let total = full.lines.len();
        assert!(total > 3, "fixture should have more than 3 context lines for this test");

        // Now cap at 3
        let capped = extract_context(&fixture, "src/event_store.rs", "EventStore", 1, Some(3));
        assert_eq!(capped.lines.len(), 3, "should return exactly 3 lines");
        assert_eq!(capped.truncated_count, total - 3, "truncated_count should reflect omitted lines");

        // Capped lines should be the first 3 from the full result
        for (a, b) in capped.lines.iter().zip(full.lines.iter()) {
            assert_eq!(a.line_number, b.line_number);
        }
    }

    #[test]
    fn max_lines_zero_means_unlimited() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/sample_repo");

        let unlimited = extract_context(&fixture, "src/event_store.rs", "EventStore", 1, None);
        let zero_cap = extract_context(&fixture, "src/event_store.rs", "EventStore", 1, Some(0));

        assert_eq!(unlimited.lines.len(), zero_cap.lines.len(), "Some(0) should behave like None");
        assert_eq!(zero_cap.truncated_count, 0);
    }
}
