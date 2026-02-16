use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use fs4::fs_std::FileExt;
use serde::{Deserialize, Serialize};

use crate::indexer::writer::utc_timestamp_iso8601;

#[derive(Serialize, Deserialize, Debug, Default, PartialEq)]
pub struct Stats {
    pub total_searches: u64,
    pub last_search_at: Option<String>,
    pub total_output_chars: u64,
    pub total_estimated_tokens: u64,
}

#[derive(Deserialize)]
struct SearchLogRecoveryEntry {
    #[serde(default)]
    ts: Option<String>,
    #[serde(default)]
    tokens: Option<u64>,
    #[serde(default)]
    outcome: Option<String>,
}

/// Reads `.ns/stats.json`, returning defaults if missing or corrupt.
pub fn read_stats(root: &Path) -> Stats {
    let from_file = read_stats_file(root);
    let from_log = recover_stats_from_search_log(root);

    match (from_file, from_log) {
        (Some(file_stats), Some(log_stats)) => merge_cumulative_stats(file_stats, log_stats),
        (Some(file_stats), None) => file_stats,
        (None, Some(log_stats)) => log_stats,
        (None, None) => Stats::default(),
    }
}

fn read_stats_file(root: &Path) -> Option<Stats> {
    let path = root.join(".ns").join("stats.json");
    fs::read_to_string(&path)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
}

fn recover_stats_from_search_log(root: &Path) -> Option<Stats> {
    let path = root.join(".ns").join("search_log.jsonl");
    let content = fs::read_to_string(path).ok()?;

    let mut stats = Stats::default();
    let mut has_success = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<SearchLogRecoveryEntry>(trimmed) else {
            continue;
        };

        // Legacy log entries (v0.1.5) had no outcome field and were success-only.
        let is_success = match entry.outcome.as_deref() {
            Some("success") | None => true,
            Some("no_results") | Some("error") => false,
            Some(_) => false,
        };

        if !is_success {
            continue;
        }

        has_success = true;
        stats.total_searches = stats.total_searches.saturating_add(1);

        if let Some(tokens) = entry.tokens {
            stats.total_estimated_tokens = stats.total_estimated_tokens.saturating_add(tokens);
            stats.total_output_chars =
                stats.total_output_chars.saturating_add(tokens.saturating_mul(4));
        }

        if let Some(ts) = entry.ts {
            stats.last_search_at = Some(ts);
        }
    }

    has_success.then_some(stats)
}

fn merge_cumulative_stats(file_stats: Stats, log_stats: Stats) -> Stats {
    Stats {
        total_searches: file_stats.total_searches.max(log_stats.total_searches),
        total_output_chars: file_stats.total_output_chars.max(log_stats.total_output_chars),
        total_estimated_tokens: file_stats
            .total_estimated_tokens
            .max(log_stats.total_estimated_tokens),
        last_search_at: latest_timestamp(file_stats.last_search_at, log_stats.last_search_at),
    }
}

fn latest_timestamp(a: Option<String>, b: Option<String>) -> Option<String> {
    match (a, b) {
        (Some(a_ts), Some(b_ts)) => {
            if a_ts >= b_ts {
                Some(a_ts)
            } else {
                Some(b_ts)
            }
        }
        (Some(a_ts), None) => Some(a_ts),
        (None, Some(b_ts)) => Some(b_ts),
        (None, None) => None,
    }
}

/// Records a search invocation. Never panics or propagates errors.
pub fn record_search(root: &Path, output_chars: usize) {
    let _ = record_search_inner(root, output_chars);
}

fn record_search_inner(root: &Path, output_chars: usize) -> Option<()> {
    let ns_dir = root.join(".ns");
    fs::create_dir_all(&ns_dir).ok()?;

    let lock_path = ns_dir.join("stats.lock");
    let lock_file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(lock_path)
        .ok()?;
    lock_file.lock_exclusive().ok()?;

    let result = (|| {
        let mut stats = read_stats(root);
        stats.total_searches += 1;
        stats.last_search_at = Some(utc_timestamp_iso8601());
        stats.total_output_chars += output_chars as u64;
        stats.total_estimated_tokens += (output_chars / 4) as u64;

        let path = ns_dir.join("stats.json");
        let json = serde_json::to_string(&stats).ok()?;
        write_atomic(&path, &json)
    })();

    let _ = lock_file.unlock();
    result
}

fn write_atomic(path: &Path, content: &str) -> Option<()> {
    let tmp_path = path.with_extension("json.tmp");
    fs::write(&tmp_path, content).ok()?;
    #[cfg(windows)]
    {
        let _ = fs::remove_file(path);
    }
    fs::rename(&tmp_path, path).ok()
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
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn stats_defaults_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
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
        // No .ns directory — should not panic and should create stats file
        record_search(dir.path(), 100);
        assert!(dir.path().join(".ns/stats.json").exists());
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
            v: "0.1.7",
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
            v: "0.1.7",
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
            v: "0.1.7",
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

    #[test]
    fn read_stats_recovers_from_legacy_success_log_when_stats_missing() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join(".ns")).unwrap();

        let line1 = serde_json::json!({
            "ts": "2026-02-16T17:00:00Z",
            "v": "0.1.5",
            "query": "A",
            "tokens": 100,
            "lines": 3,
            "files": 1,
            "mode": "text",
            "budget": null
        });
        let line2 = serde_json::json!({
            "ts": "2026-02-16T17:05:00Z",
            "v": "0.1.5",
            "query": "B",
            "tokens": 50,
            "lines": 2,
            "files": 1,
            "mode": "text",
            "budget": null
        });
        fs::write(
            root.join(".ns/search_log.jsonl"),
            format!("{}\n{}\n", line1, line2),
        )
        .unwrap();

        let stats = read_stats(root);
        assert_eq!(stats.total_searches, 2);
        assert_eq!(stats.total_estimated_tokens, 150);
        assert_eq!(stats.total_output_chars, 600);
        assert_eq!(
            stats.last_search_at.as_deref(),
            Some("2026-02-16T17:05:00Z")
        );
    }

    #[test]
    fn read_stats_uses_higher_cumulative_totals_from_log() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join(".ns")).unwrap();

        // Simulate a reset: stats.json has lower valid totals.
        let low_stats = Stats {
            total_searches: 11,
            last_search_at: Some("2026-02-16T17:06:22Z".to_string()),
            total_output_chars: 5793,
            total_estimated_tokens: 1444,
        };
        fs::write(
            root.join(".ns/stats.json"),
            serde_json::to_string(&low_stats).unwrap(),
        )
        .unwrap();

        let success = serde_json::json!({
            "ts": "2026-02-16T18:00:00Z",
            "tokens": 50000,
            "outcome": "success"
        });
        fs::write(root.join(".ns/search_log.jsonl"), format!("{}\n", success)).unwrap();

        let stats = read_stats(root);
        assert_eq!(stats.total_searches, 11.max(1));
        assert_eq!(stats.total_estimated_tokens, 50_000);
        assert_eq!(stats.total_output_chars, 200_000);
        assert_eq!(
            stats.last_search_at.as_deref(),
            Some("2026-02-16T18:00:00Z")
        );
    }

    #[test]
    fn read_stats_recovers_only_success_entries_from_v2_log() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join(".ns")).unwrap();

        let success = serde_json::json!({
            "ts": "2026-02-16T17:00:00Z",
            "tokens": 100,
            "outcome": "success"
        });
        let no_results = serde_json::json!({
            "ts": "2026-02-16T17:01:00Z",
            "tokens": 10,
            "outcome": "no_results"
        });
        let error = serde_json::json!({
            "ts": "2026-02-16T17:02:00Z",
            "tokens": 0,
            "outcome": "error"
        });
        fs::write(
            root.join(".ns/search_log.jsonl"),
            format!("{}\n{}\n{}\n", success, no_results, error),
        )
        .unwrap();

        let stats = read_stats(root);
        assert_eq!(stats.total_searches, 1);
        assert_eq!(stats.total_estimated_tokens, 100);
        assert_eq!(stats.total_output_chars, 400);
        assert_eq!(
            stats.last_search_at.as_deref(),
            Some("2026-02-16T17:00:00Z")
        );
    }

    #[test]
    fn record_search_recovers_from_corrupt_stats_and_increments() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join(".ns")).unwrap();

        fs::write(root.join(".ns/stats.json"), "{not-json").unwrap();

        let line1 = serde_json::json!({
            "ts": "2026-02-16T17:00:00Z",
            "tokens": 10,
            "outcome": "success"
        });
        let line2 = serde_json::json!({
            "ts": "2026-02-16T17:01:00Z",
            "tokens": 20,
            "outcome": "success"
        });
        fs::write(
            root.join(".ns/search_log.jsonl"),
            format!("{}\n{}\n", line1, line2),
        )
        .unwrap();

        record_search(root, 400);
        let stats = read_stats(root);
        assert_eq!(stats.total_searches, 3);
        assert_eq!(stats.total_estimated_tokens, 130);
        assert_eq!(stats.total_output_chars, 520);
    }

    #[test]
    fn record_search_is_cumulative_under_concurrency() {
        let dir = tempfile::tempdir().unwrap();
        let root = Arc::new(dir.path().to_path_buf());
        fs::create_dir_all(root.join(".ns")).unwrap();

        let workers = 12;
        let per_worker = 80;
        let mut handles = Vec::new();

        for _ in 0..workers {
            let root = Arc::clone(&root);
            handles.push(thread::spawn(move || {
                for _ in 0..per_worker {
                    record_search(&root, 40); // 10 estimated tokens
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        let stats = read_stats(&root);
        let expected = (workers * per_worker) as u64;
        assert_eq!(stats.total_searches, expected);
        assert_eq!(stats.total_estimated_tokens, expected * 10);
        assert_eq!(stats.total_output_chars, expected * 40);
    }
}
