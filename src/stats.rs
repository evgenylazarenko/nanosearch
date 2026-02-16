use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::indexer::writer::utc_timestamp_iso8601;

#[derive(Serialize, Deserialize, Debug, Default, PartialEq)]
pub struct Stats {
    pub total_searches: u64,
    pub last_search_at: Option<String>,
    pub total_output_chars: u64,
    pub total_estimated_tokens: u64,
}

/// Reads `.ns/stats.json`, returning defaults if missing or corrupt.
pub fn read_stats(root: &Path) -> Stats {
    let path = root.join(".ns").join("stats.json");
    fs::read_to_string(&path)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

/// Records a search invocation. Never panics or propagates errors.
pub fn record_search(root: &Path, output_chars: usize) {
    let _ = record_search_inner(root, output_chars);
}

fn record_search_inner(root: &Path, output_chars: usize) -> Option<()> {
    let mut stats = read_stats(root);
    stats.total_searches += 1;
    stats.last_search_at = Some(utc_timestamp_iso8601());
    stats.total_output_chars += output_chars as u64;
    stats.total_estimated_tokens += (output_chars / 4) as u64;

    let path = root.join(".ns").join("stats.json");
    let json = serde_json::to_string(&stats).ok()?;
    fs::write(&path, json).ok()
}

#[derive(Serialize)]
pub struct SearchLogEntry {
    pub ts: String,
    pub v: &'static str,
    pub query: String,
    pub tokens: usize,
    pub lines: usize,
    pub files: usize,
    pub mode: String,
    pub budget: Option<usize>,
    pub outcome: SearchOutcome,
    pub zero_results: bool,
    pub flags: SearchLogFlags,
    pub argv: Vec<String>,
    pub error: Option<SearchLogError>,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchOutcome {
    Success,
    NoResults,
    Error,
}

#[derive(Serialize)]
pub struct SearchLogFlags {
    pub file_type: Option<String>,
    pub file_glob: Option<String>,
    pub files_only: bool,
    pub ignore_case: bool,
    pub json: bool,
    pub sym: bool,
    pub fuzzy: bool,
    pub max_count: usize,
    pub context: usize,
    pub max_context_lines: usize,
    pub budget: Option<usize>,
}

#[derive(Serialize)]
pub struct SearchLogError {
    pub code: &'static str,
    pub message: String,
}

/// Appends one JSON line to `.ns/search_log.jsonl`. Fire-and-forget.
pub fn record_search_log(root: &Path, entry: SearchLogEntry) {
    let _ = record_search_log_inner(root, &entry);
}

fn record_search_log_inner(root: &Path, entry: &SearchLogEntry) -> Option<()> {
    let path = root.join(".ns").join("search_log.jsonl");
    fs::create_dir_all(path.parent()?).ok()?;
    let line = serde_json::to_string(entry).ok()?;
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .ok()?;
    writeln!(f, "{}", line).ok()
}

pub fn format_token_count(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("~{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("~{:.1}k", tokens as f64 / 1_000.0)
    } else {
        format!("{}", tokens)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn stats_defaults_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join(".ns")).unwrap();
        let stats = read_stats(root);
        assert_eq!(stats, Stats::default());
    }

    #[test]
    fn record_and_read_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join(".ns")).unwrap();

        record_search(root, 400);
        let stats = read_stats(root);
        assert_eq!(stats.total_searches, 1);
        assert_eq!(stats.total_output_chars, 400);
        assert_eq!(stats.total_estimated_tokens, 100);
        assert!(stats.last_search_at.is_some());

        record_search(root, 200);
        let stats = read_stats(root);
        assert_eq!(stats.total_searches, 2);
        assert_eq!(stats.total_output_chars, 600);
        assert_eq!(stats.total_estimated_tokens, 150);
    }

    #[test]
    fn record_silent_on_missing_ns_dir() {
        let dir = tempfile::tempdir().unwrap();
        // No .ns directory — should not panic
        record_search(dir.path(), 100);
    }

    #[test]
    fn format_token_count_ranges() {
        assert_eq!(format_token_count(500), "500");
        assert_eq!(format_token_count(21355), "~21.4k");
        assert_eq!(format_token_count(1_500_000), "~1.5M");
    }

    #[test]
    fn search_log_appends_valid_jsonl() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join(".ns")).unwrap();

        let entry1 = SearchLogEntry {
            ts: "2026-02-13T10:30:00Z".to_string(),
            v: "0.1.6",
            query: "EventStore".to_string(),
            tokens: 84,
            lines: 12,
            files: 2,
            mode: "text".to_string(),
            budget: None,
            outcome: SearchOutcome::Success,
            zero_results: false,
            flags: SearchLogFlags {
                file_type: None,
                file_glob: None,
                files_only: false,
                ignore_case: false,
                json: false,
                sym: false,
                fuzzy: false,
                max_count: 10,
                context: 1,
                max_context_lines: 30,
                budget: None,
            },
            argv: vec!["--".to_string(), "EventStore".to_string()],
            error: None,
        };
        record_search_log(root, entry1);

        let entry2 = SearchLogEntry {
            ts: "2026-02-13T10:31:00Z".to_string(),
            v: "0.1.6",
            query: "Validator".to_string(),
            tokens: 40,
            lines: 5,
            files: 1,
            mode: "json".to_string(),
            budget: Some(500),
            outcome: SearchOutcome::NoResults,
            zero_results: true,
            flags: SearchLogFlags {
                file_type: Some("rust".to_string()),
                file_glob: Some("src/*.rs".to_string()),
                files_only: false,
                ignore_case: true,
                json: true,
                sym: false,
                fuzzy: false,
                max_count: 5,
                context: 0,
                max_context_lines: 10,
                budget: Some(500),
            },
            argv: vec![
                "--json".to_string(),
                "-t".to_string(),
                "rust".to_string(),
                "Validator".to_string(),
            ],
            error: Some(SearchLogError {
                code: "invalid_query",
                message: "invalid query".to_string(),
            }),
        };
        record_search_log(root, entry2);

        let content = fs::read_to_string(root.join(".ns/search_log.jsonl")).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);

        let v1: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(v1["query"], "EventStore");
        assert_eq!(v1["budget"], serde_json::Value::Null);
        assert_eq!(v1["outcome"], "success");
        assert_eq!(v1["zero_results"], false);
        assert!(v1["error"].is_null());
        assert_eq!(v1["flags"]["max_count"], 10);
        assert_eq!(v1["argv"][1], "EventStore");

        let v2: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(v2["query"], "Validator");
        assert_eq!(v2["budget"], 500);
        assert_eq!(v2["outcome"], "no_results");
        assert_eq!(v2["zero_results"], true);
        assert_eq!(v2["flags"]["file_type"], "rust");
        assert_eq!(v2["error"]["code"], "invalid_query");
    }

    #[test]
    fn search_log_creates_ns_dir_if_missing() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let entry = SearchLogEntry {
            ts: "2026-02-13T10:30:00Z".to_string(),
            v: "0.1.6",
            query: "EventStore".to_string(),
            tokens: 84,
            lines: 12,
            files: 2,
            mode: "text".to_string(),
            budget: None,
            outcome: SearchOutcome::Error,
            zero_results: false,
            flags: SearchLogFlags {
                file_type: None,
                file_glob: None,
                files_only: false,
                ignore_case: false,
                json: false,
                sym: false,
                fuzzy: false,
                max_count: 10,
                context: 1,
                max_context_lines: 30,
                budget: None,
            },
            argv: vec!["EventStore".to_string()],
            error: Some(SearchLogError {
                code: "no_index",
                message: "no index found".to_string(),
            }),
        };

        record_search_log(root, entry);
        let log_path = root.join(".ns/search_log.jsonl");
        assert!(log_path.exists(), "search log file should be created");
    }

    #[test]
    fn serde_round_trip() {
        let stats = Stats {
            total_searches: 42,
            last_search_at: Some("2026-02-13T10:30:00Z".to_string()),
            total_output_chars: 8000,
            total_estimated_tokens: 2000,
        };
        let json = serde_json::to_string(&stats).unwrap();
        let parsed: Stats = serde_json::from_str(&json).unwrap();
        assert_eq!(stats, parsed);
    }
}
