use std::collections::BTreeSet;
use std::path::Path;

/// A single line from a matched file, with its 1-based line number.
pub struct ContextLine {
    pub line_number: usize,
    pub text: String,
}

/// Extracts context lines from a file that matched a search query.
///
/// For each query term, finds all lines containing it (case-insensitive),
/// then expands each match by ±`context_window` lines. Overlapping ranges
/// are merged. Returns lines sorted by line number.
///
/// If the file cannot be read (deleted/moved since indexing), returns an empty vec.
pub fn extract_context(
    root: &Path,
    rel_path: &str,
    query: &str,
    context_window: usize,
) -> Vec<ContextLine> {
    let full_path = root.join(rel_path);
    let content = match std::fs::read_to_string(&full_path) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();
    if total_lines == 0 {
        return Vec::new();
    }

    // Tokenize the query by splitting on non-alphanumeric boundaries, then lowercase.
    // This mirrors tantivy's default tokenizer behavior — e.g. "EventStore.new" becomes
    // ["eventstore", "new"], "HashMap<String>" becomes ["hashmap", "string"].
    let terms: Vec<String> = tokenize_query(query);

    if terms.is_empty() {
        return Vec::new();
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
        return Vec::new();
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

    // Build context lines (1-based line numbers)
    include_indices
        .iter()
        .map(|&i| ContextLine {
            line_number: i + 1,
            text: lines[i].to_string(),
        })
        .collect()
}

/// Tokenizes a query string the same way tantivy's default tokenizer does:
/// split on non-alphanumeric boundaries, lowercase each token, drop empties.
fn tokenize_query(query: &str) -> Vec<String> {
    query
        .split(|c: char| !c.is_alphanumeric() && c != '_')
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
        let lines = extract_context(&fixture, "src/event_store.rs", "EventStore", 1);

        assert!(!lines.is_empty(), "should find lines matching EventStore");

        // Verify we got the struct definition line
        let has_struct = lines.iter().any(|l| l.text.contains("pub struct EventStore"));
        assert!(has_struct, "should find 'pub struct EventStore' line");

        // Verify line numbers are in order
        for window in lines.windows(2) {
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

        let lines = extract_context(&fixture, "src/event_store.rs", "EventStore", 0);

        // Every returned line should contain "EventStore" (case-insensitive)
        for line in &lines {
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

        let lines = extract_context(&fixture, "nonexistent.rs", "anything", 1);
        assert!(lines.is_empty(), "missing file should return empty vec");
    }

    #[test]
    fn tokenize_splits_on_punctuation() {
        let terms = tokenize_query("EventStore.new");
        assert_eq!(terms, vec!["eventstore", "new"]);

        let terms = tokenize_query("HashMap<String>");
        assert_eq!(terms, vec!["hashmap", "string"]);

        let terms = tokenize_query("foo::bar_baz");
        assert_eq!(terms, vec!["foo", "bar_baz"]);

        let terms = tokenize_query("  spaced   out  ");
        assert_eq!(terms, vec!["spaced", "out"]);
    }

    #[test]
    fn multiterm_query_matches() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/sample_repo");

        // "validate port" — both terms appear in validator.rs
        let lines = extract_context(&fixture, "src/validator.rs", "validate port", 0);

        // Should find lines containing either "validate" or "port"
        assert!(!lines.is_empty(), "should find lines for multi-term query");
    }
}
