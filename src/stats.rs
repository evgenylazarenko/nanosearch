use std::fs;
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
    let json = serde_json::to_string_pretty(&stats).ok()?;
    fs::write(&path, json).ok()
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
