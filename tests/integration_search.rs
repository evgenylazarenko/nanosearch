mod common;

use ns::searcher::query::SearchOptions;
use ns::searcher::OutputMode;

/// Helper to build default SearchOptions with a given max_results.
fn opts(max: usize) -> SearchOptions {
    SearchOptions {
        max_results: max,
        ..Default::default()
    }
}

#[test]
fn search_returns_ranked_results() {
    let (_tmp, root) = common::indexed_fixture();

    let (results, stats) =
        ns::searcher::query::execute_search(&root, "EventStore", &opts(10))
            .expect("search should work");

    assert!(!results.is_empty(), "should find results for 'EventStore'");
    assert_eq!(stats.total_results, results.len());
    assert!(stats.files_searched > 0);

    // event_store.rs should rank first (it defines EventStore)
    assert!(
        results[0].path.contains("event_store.rs"),
        "event_store.rs should rank first, got: {}",
        results[0].path
    );
    assert_eq!(results[0].lang.as_deref(), Some("rust"));
    assert!(results[0].score > 0.0);
}

#[test]
fn search_full_pipeline_with_context() {
    let (_tmp, root) = common::indexed_fixture();

    let so =
        ns::searcher::search(&root, "EventStore", OutputMode::Text, &SearchOptions::default())
            .expect("search pipeline should work");

    assert!(so.formatted.contains("[1]"), "should have result rank [1]");
    assert!(so.formatted.contains("event_store.rs"), "should show event_store.rs");
    assert!(so.formatted.contains("score:"), "should show score");
    assert!(so.formatted.contains("lang: rust"), "should show lang");
    // Summary is on stderr (via format_summary), not in the library output
    assert!(!so.formatted.contains("result (searched"), "summary should not be in library output");
    assert!(so.stats.total_results > 0);
}

#[test]
fn search_multiterm_query() {
    let (_tmp, root) = common::indexed_fixture();

    let (results, _stats) =
        ns::searcher::query::execute_search(&root, "validate port", &opts(10))
            .expect("search should work");

    assert!(!results.is_empty(), "should find results for multi-term query");
    let has_validator = results.iter().any(|r| r.path.contains("validator.rs"));
    assert!(has_validator, "validator.rs should appear in results");
}

#[test]
fn search_no_results_returns_empty() {
    let (_tmp, root) = common::indexed_fixture();

    let (results, stats) =
        ns::searcher::query::execute_search(&root, "xyzzy_nonexistent_term", &opts(10))
            .expect("search should succeed even with no matches");

    assert!(results.is_empty(), "should find no results for nonsense term");
    assert_eq!(stats.total_results, 0);
}

#[test]
fn search_respects_max_results() {
    let (_tmp, root) = common::indexed_fixture();

    let (results, _stats) =
        ns::searcher::query::execute_search(&root, "fn", &opts(2))
            .expect("search should work");

    assert!(
        results.len() <= 2,
        "should return at most 2 results, got {}",
        results.len()
    );
}

#[test]
fn search_context_lines_are_present() {
    let (_tmp, root) = common::indexed_fixture();

    let so =
        ns::searcher::search(
            &root,
            "EventStore",
            OutputMode::Text,
            &SearchOptions { max_results: 5, ..Default::default() },
        )
        .expect("search should work");

    assert!(
        so.formatted.contains("pub struct EventStore"),
        "output should contain context line with struct definition"
    );
}

#[test]
fn search_context_shows_separators_between_groups() {
    let (_tmp, root) = common::indexed_fixture();

    // EventStore appears on multiple non-contiguous lines in event_store.rs
    let so =
        ns::searcher::search(
            &root,
            "EventStore",
            OutputMode::Text,
            &SearchOptions { max_results: 5, context_window: 0, ..Default::default() },
        )
        .expect("search should work");

    // With context=0 and multiple match locations, there should be group separators
    assert!(
        so.formatted.contains("..."),
        "non-contiguous context groups should be separated by '...'"
    );
}

// ── Phase 4: Symbol extraction + boost tests ─────────────────────────────────

#[test]
fn symbol_boost_ranks_definition_file_first() {
    let (_tmp, root) = common::indexed_fixture();

    let (results, _stats) =
        ns::searcher::query::execute_search(&root, "Router", &opts(10))
            .expect("search should work");

    assert!(!results.is_empty(), "should find results for 'Router'");
    assert!(
        results[0].path.contains("handlers.ts"),
        "handlers.ts (defines Router class) should rank first, got: {}",
        results[0].path
    );
}

#[test]
fn symbol_boost_ranks_struct_definition_higher() {
    let (_tmp, root) = common::indexed_fixture();

    let (results, _stats) =
        ns::searcher::query::execute_search(&root, "EventStore", &opts(10))
            .expect("search should work");

    assert!(!results.is_empty());
    assert!(
        results[0].path.contains("event_store.rs"),
        "event_store.rs (defines EventStore struct) should rank first, got: {}",
        results[0].path
    );
}

#[test]
fn symbol_search_finds_python_classes() {
    let (_tmp, root) = common::indexed_fixture();

    let (results, _stats) =
        ns::searcher::query::execute_search(&root, "UserRepository", &opts(10))
            .expect("search should work");

    assert!(!results.is_empty(), "should find results for 'UserRepository'");
    assert!(
        results[0].path.contains("models.py"),
        "models.py (defines UserRepository class) should rank first, got: {}",
        results[0].path
    );
}

#[test]
fn symbol_search_finds_go_types() {
    let (_tmp, root) = common::indexed_fixture();

    let (results, _stats) =
        ns::searcher::query::execute_search(&root, "ServerConfig", &opts(10))
            .expect("search should work");

    assert!(!results.is_empty(), "should find results for 'ServerConfig'");
    assert!(
        results[0].path.contains("server.go"),
        "server.go (defines ServerConfig type) should rank first, got: {}",
        results[0].path
    );
}

#[test]
fn symbol_search_finds_js_functions() {
    let (_tmp, root) = common::indexed_fixture();

    let (results, _stats) =
        ns::searcher::query::execute_search(&root, "debounce", &opts(10))
            .expect("search should work");

    assert!(!results.is_empty(), "should find results for 'debounce'");
    assert!(
        results[0].path.contains("utils.js"),
        "utils.js (defines debounce function) should rank first, got: {}",
        results[0].path
    );
}

#[test]
fn symbol_search_finds_elixir_modules_and_functions() {
    let (_tmp, root) = common::indexed_fixture();

    let (results, _stats) =
        ns::searcher::query::execute_search(&root, "EventManager", &opts(10))
            .expect("search should work");

    assert!(!results.is_empty(), "should find results for 'EventManager'");
    assert!(
        results[0].path.contains("event_manager.ex"),
        "event_manager.ex (defines EventManager module) should rank first, got: {}",
        results[0].path
    );
}

#[test]
fn symbol_search_finds_elixir_protocol() {
    let (_tmp, root) = common::indexed_fixture();

    let (results, _stats) =
        ns::searcher::query::execute_search(&root, "Publishable", &opts(10))
            .expect("search should work");

    assert!(!results.is_empty(), "should find results for 'Publishable'");
    let has_elixir = results.iter().any(|r| r.path.contains("event_manager.ex"));
    assert!(
        has_elixir,
        "event_manager.ex should appear in results for 'Publishable'"
    );
}

// ── Phase 5: Filters, flags, output modes ─────────────────────────────────────

#[test]
fn filter_by_language_type() {
    let (_tmp, root) = common::indexed_fixture();

    let rust_opts = SearchOptions {
        max_results: 10,
        file_type: Some("rust".to_string()),
        ..Default::default()
    };
    let (results, _stats) =
        ns::searcher::query::execute_search(&root, "fn", &rust_opts)
            .expect("search should work");

    assert!(!results.is_empty(), "should find Rust files matching 'fn'");
    for r in &results {
        assert_eq!(
            r.lang.as_deref(),
            Some("rust"),
            "all results should be rust, got: {:?} for {}",
            r.lang,
            r.path
        );
    }
}

#[test]
fn filter_by_language_excludes_other_langs() {
    let (_tmp, root) = common::indexed_fixture();

    let py_opts = SearchOptions {
        max_results: 10,
        file_type: Some("python".to_string()),
        ..Default::default()
    };
    let (results, _stats) =
        ns::searcher::query::execute_search(&root, "class", &py_opts)
            .expect("search should work");

    for r in &results {
        assert_eq!(
            r.lang.as_deref(),
            Some("python"),
            "should only return python files, got: {:?} for {}",
            r.lang,
            r.path
        );
    }
}

#[test]
fn glob_filter_restricts_paths() {
    let (_tmp, root) = common::indexed_fixture();

    let glob_opts = SearchOptions {
        max_results: 10,
        file_glob: Some("src/*.rs".to_string()),
        ..Default::default()
    };
    let (results, _stats) =
        ns::searcher::query::execute_search(&root, "fn", &glob_opts)
            .expect("search should work");

    assert!(!results.is_empty(), "should find results matching glob");
    for r in &results {
        assert!(
            r.path.starts_with("src/") && r.path.ends_with(".rs"),
            "path should match src/*.rs, got: {}",
            r.path
        );
    }
}

#[test]
fn invalid_glob_returns_error() {
    let (_tmp, root) = common::indexed_fixture();

    let bad_glob_opts = SearchOptions {
        max_results: 10,
        file_glob: Some("[invalid".to_string()),
        ..Default::default()
    };
    let result = ns::searcher::query::execute_search(&root, "fn", &bad_glob_opts);
    assert!(result.is_err(), "invalid glob should return an error");
}

#[test]
fn files_only_output_bare_paths() {
    let (_tmp, root) = common::indexed_fixture();

    let so =
        ns::searcher::search(&root, "EventStore", OutputMode::FilesOnly, &SearchOptions::default())
            .expect("search should work");

    assert!(so.stats.total_results > 0);
    // Output should be bare paths — no scores, no context, no brackets
    assert!(!so.formatted.contains("[1]"), "files-only should not have rank markers");
    assert!(!so.formatted.contains("score:"), "files-only should not have scores");
    // Each line should be a file path
    for line in so.formatted.lines() {
        assert!(
            line.contains('.'),
            "each line should be a file path, got: {}",
            line
        );
    }
}

#[test]
fn symbol_only_search() {
    let (_tmp, root) = common::indexed_fixture();

    let sym_opts = SearchOptions {
        max_results: 10,
        sym_only: true,
        ..Default::default()
    };
    let (results, _stats) =
        ns::searcher::query::execute_search(&root, "EventStore", &sym_opts)
            .expect("search should work");

    assert!(!results.is_empty(), "should find symbol matches for 'EventStore'");
    // event_store.rs defines the EventStore symbol
    assert!(
        results[0].path.contains("event_store.rs"),
        "event_store.rs should rank first in symbol-only search, got: {}",
        results[0].path
    );
}

#[test]
fn symbol_only_excludes_content_only_matches() {
    let (_tmp, root) = common::indexed_fixture();

    let sym_opts = SearchOptions {
        max_results: 10,
        sym_only: true,
        ..Default::default()
    };
    let (results, _stats) =
        ns::searcher::query::execute_search(&root, "xyzzy_only_in_comments", &sym_opts)
            .expect("search should work");

    assert!(results.is_empty(), "symbol-only search should not match content-only terms");
}

#[test]
fn json_output_is_valid_json() {
    let (_tmp, root) = common::indexed_fixture();

    let so =
        ns::searcher::search(&root, "EventStore", OutputMode::Json, &SearchOptions::default())
            .expect("search should work");

    assert!(so.stats.total_results > 0);
    let parsed: serde_json::Value =
        serde_json::from_str(&so.formatted).expect("output should be valid JSON");

    assert_eq!(parsed["query"], "EventStore");
    assert!(parsed["results"].is_array());
    assert!(parsed["results"].as_array().unwrap().len() > 0);

    let first = &parsed["results"][0];
    assert!(first["path"].is_string());
    assert!(first["score"].is_number());
    assert!(first["matched_symbols"].is_array());
    assert!(first["lines"].is_array());
    assert!(parsed["stats"]["total_results"].is_number());
}

#[test]
fn json_output_has_matched_symbols() {
    let (_tmp, root) = common::indexed_fixture();

    let so =
        ns::searcher::search(&root, "EventStore", OutputMode::Json, &SearchOptions::default())
            .expect("search should work");

    let parsed: serde_json::Value = serde_json::from_str(&so.formatted).unwrap();

    // The first result (event_store.rs) should have EventStore in matched_symbols
    let first = &parsed["results"][0];
    let matched = first["matched_symbols"].as_array().unwrap();
    let has_event_store = matched.iter().any(|v| v.as_str() == Some("EventStore"));
    assert!(
        has_event_store,
        "matched_symbols should contain 'EventStore', got: {:?}",
        matched
    );
}

#[test]
fn json_output_lines_use_num_field() {
    let (_tmp, root) = common::indexed_fixture();

    let so =
        ns::searcher::search(&root, "EventStore", OutputMode::Json, &SearchOptions::default())
            .expect("search should work");

    let parsed: serde_json::Value = serde_json::from_str(&so.formatted).unwrap();
    let first = &parsed["results"][0];
    let lines = first["lines"].as_array().unwrap();
    assert!(!lines.is_empty(), "should have context lines");
    // Each line entry should have "num" and "text" fields (matching concept doc spec)
    assert!(lines[0]["num"].is_number(), "line entries should have 'num' field");
    assert!(lines[0]["text"].is_string(), "line entries should have 'text' field");
}

#[test]
fn fuzzy_search_finds_typo() {
    let (_tmp, root) = common::indexed_fixture();

    let fuzzy_opts = SearchOptions {
        max_results: 10,
        fuzzy: true,
        ..Default::default()
    };
    // "EvntStore" is one deletion away from "EventStore"
    let (results, _stats) =
        ns::searcher::query::execute_search(&root, "EvntStore", &fuzzy_opts)
            .expect("fuzzy search should work");

    assert!(!results.is_empty(), "fuzzy search should find results for 'EvntStore'");
    let has_event_store = results.iter().any(|r| r.path.contains("event_store.rs"));
    assert!(
        has_event_store,
        "fuzzy search should find event_store.rs for 'EvntStore'"
    );
}

#[test]
fn case_insensitive_search_matches() {
    let (_tmp, root) = common::indexed_fixture();

    // Search lowercase — should match PascalCase symbol "EventStore"
    let (results_lower, _) =
        ns::searcher::query::execute_search(&root, "eventstore", &opts(10))
            .expect("search should work");

    let (results_pascal, _) =
        ns::searcher::query::execute_search(&root, "EventStore", &opts(10))
            .expect("search should work");

    assert!(!results_lower.is_empty(), "lowercase query should find results");
    assert!(!results_pascal.is_empty(), "PascalCase query should find results");

    // Both should find event_store.rs
    assert!(results_lower.iter().any(|r| r.path.contains("event_store.rs")));
    assert!(results_pascal.iter().any(|r| r.path.contains("event_store.rs")));
}

#[test]
fn language_filter_with_fuzzy() {
    let (_tmp, root) = common::indexed_fixture();

    let fuzzy_rust_opts = SearchOptions {
        max_results: 10,
        file_type: Some("rust".to_string()),
        fuzzy: true,
        ..Default::default()
    };
    let (results, _stats) =
        ns::searcher::query::execute_search(&root, "EvntStore", &fuzzy_rust_opts)
            .expect("search should work");

    for r in &results {
        assert_eq!(
            r.lang.as_deref(),
            Some("rust"),
            "fuzzy + lang filter should only return rust files, got: {:?}",
            r.lang
        );
    }
}

#[test]
fn language_filter_with_sym_only() {
    let (_tmp, root) = common::indexed_fixture();

    let sym_rust_opts = SearchOptions {
        max_results: 10,
        file_type: Some("rust".to_string()),
        sym_only: true,
        ..Default::default()
    };
    let (results, _stats) =
        ns::searcher::query::execute_search(&root, "EventStore", &sym_rust_opts)
            .expect("search should work");

    assert!(!results.is_empty());
    for r in &results {
        assert_eq!(
            r.lang.as_deref(),
            Some("rust"),
            "sym + lang filter should only return rust files, got: {:?}",
            r.lang
        );
    }
}

// ── Phase 7: Error handling + polish tests ────────────────────────────────────

// Library-level error path tests

#[test]
fn search_without_index_returns_error() {
    let (_tmp, root) = common::isolated_fixture();

    let result = ns::searcher::search(
        &root,
        "anything",
        OutputMode::Text,
        &SearchOptions::default(),
    );
    assert!(result.is_err(), "search without index should fail");
}

#[test]
fn search_with_stale_schema_version_returns_error() {
    let (_tmp, root) = common::indexed_fixture();

    // Tamper with meta.json to simulate a stale schema version
    let meta_path = root.join(".ns").join("meta.json");
    let content = std::fs::read_to_string(&meta_path).expect("should read meta");
    let tampered = content.replace("\"schema_version\": 2", "\"schema_version\": 999");
    std::fs::write(&meta_path, &tampered).expect("should write tampered meta");

    let result = ns::searcher::search(
        &root,
        "EventStore",
        OutputMode::Text,
        &SearchOptions::default(),
    );

    assert!(result.is_err(), "search with stale schema should fail");
    let err = result.unwrap_err();
    let msg = format!("{}", err);
    assert!(
        msg.contains("schema version"),
        "error should mention schema version, got: {}",
        msg
    );
}

#[test]
fn glob_and_language_filter_combined() {
    let (_tmp, root) = common::indexed_fixture();

    let combo_opts = SearchOptions {
        max_results: 10,
        file_type: Some("rust".to_string()),
        file_glob: Some("src/event_*".to_string()),
        ..Default::default()
    };
    let (results, _stats) =
        ns::searcher::query::execute_search(&root, "pub", &combo_opts)
            .expect("search should work");

    for r in &results {
        assert_eq!(r.lang.as_deref(), Some("rust"), "should be rust");
        assert!(
            r.path.starts_with("src/event_"),
            "path should match glob, got: {}",
            r.path
        );
    }
}

#[test]
fn context_window_zero_shows_only_matching_lines() {
    let (_tmp, root) = common::indexed_fixture();

    let opts_c0 = SearchOptions {
        max_results: 5,
        context_window: 0,
        ..Default::default()
    };
    let so =
        ns::searcher::search(&root, "EventStore", OutputMode::Text, &opts_c0)
            .expect("search should work");

    // With context=0, every context line should contain the query term (case-insensitive)
    for line in so.formatted.lines() {
        // Skip rank headers, separators, summary, and blank lines
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with('[')
            || trimmed.starts_with("...")
            || trimmed.starts_with('~')
            || trimmed.contains("result")
        {
            continue;
        }
        // This is a context line — it should contain "EventStore" (case-insensitive)
        assert!(
            trimmed.to_lowercase().contains("eventstore"),
            "with context=0, line should match query, got: {}",
            trimmed
        );
    }
}

#[test]
fn context_window_larger_shows_more_lines() {
    let (_tmp, root) = common::indexed_fixture();

    let opts_c0 = SearchOptions {
        max_results: 1,
        context_window: 0,
        ..Default::default()
    };
    let opts_c3 = SearchOptions {
        max_results: 1,
        context_window: 3,
        ..Default::default()
    };

    let so_c0 =
        ns::searcher::search(&root, "EventStore", OutputMode::Text, &opts_c0)
            .expect("search should work");
    let so_c3 =
        ns::searcher::search(&root, "EventStore", OutputMode::Text, &opts_c3)
            .expect("search should work");

    // Context=3 should produce more lines than context=0
    let lines_c0: Vec<&str> = so_c0.formatted.lines().filter(|l| l.trim().contains(':')).collect();
    let lines_c3: Vec<&str> = so_c3.formatted.lines().filter(|l| l.trim().contains(':')).collect();

    assert!(
        lines_c3.len() >= lines_c0.len(),
        "context=3 should show at least as many lines as context=0 ({} vs {})",
        lines_c3.len(),
        lines_c0.len()
    );
}

#[test]
fn end_to_end_index_incremental_search() {
    let (_tmp, root) = common::isolated_fixture();

    // Full index
    ns::indexer::run_full_index(&root, 1_048_576).expect("full index should succeed");

    // Verify search works
    let (results, _) =
        ns::searcher::query::execute_search(&root, "EventStore", &opts(10))
            .expect("search should work after full index");
    assert!(!results.is_empty());

    // Sleep required: without a git repo, incremental indexing uses mtime-based
    // change detection. The new file needs a later mtime than indexed_at, and
    // filesystem mtime granularity may be 1 second on some platforms.
    std::thread::sleep(std::time::Duration::from_secs(1));
    std::fs::write(
        root.join("src").join("phase7_test.rs"),
        "pub struct Phase7Marker;\n",
    )
    .expect("write should succeed");

    // Incremental index
    let stats = ns::indexer::run_incremental_index(&root, 1_048_576)
        .expect("incremental should succeed");
    assert!(stats.added >= 1, "should detect added file");

    // Verify new content is searchable
    let (results, _) =
        ns::searcher::query::execute_search(&root, "Phase7Marker", &opts(10))
            .expect("search should work after incremental");
    assert!(
        !results.is_empty(),
        "Phase7Marker should be searchable after incremental index"
    );

    // Original content still searchable
    let (results, _) =
        ns::searcher::query::execute_search(&root, "EventStore", &opts(10))
            .expect("search should still work for original content");
    assert!(
        !results.is_empty(),
        "EventStore should still be searchable after incremental"
    );
}

#[test]
fn no_results_text_returns_empty_output() {
    let (_tmp, root) = common::indexed_fixture();

    let so = ns::searcher::search(
        &root,
        "xyzzy_nonexistent_42",
        OutputMode::Text,
        &SearchOptions::default(),
    )
    .expect("search should succeed even with no results");

    assert_eq!(so.stats.total_results, 0);
    assert!(
        so.formatted.is_empty(),
        "library text output should be empty when no results (summary is CLI-layer concern)"
    );
}

#[test]
fn no_results_json_has_empty_array() {
    let (_tmp, root) = common::indexed_fixture();

    let so = ns::searcher::search(
        &root,
        "xyzzy_nonexistent_42",
        OutputMode::Json,
        &SearchOptions::default(),
    )
    .expect("search should succeed even with no results");

    assert_eq!(so.stats.total_results, 0);
    let parsed: serde_json::Value = serde_json::from_str(&so.formatted).unwrap();
    assert_eq!(parsed["results"].as_array().unwrap().len(), 0);
    assert_eq!(parsed["stats"]["total_results"], 0);
}

// ── CLI-level tests (verify actual stderr messages and exit codes) ────────────

/// Helper: get the path to the built `ns` binary.
fn ns_binary() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_ns"))
}

#[test]
fn cli_search_without_index_stderr_and_exit_code() {
    let (_tmp, root) = common::isolated_fixture();

    let output = std::process::Command::new(ns_binary())
        .arg("anything")
        .current_dir(&root)
        .output()
        .expect("should run ns binary");

    assert!(!output.status.success(), "should exit with non-zero status");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("error: no index found. Run 'ns index' to create one."),
        "stderr should have the exact no-index message, got: {}",
        stderr
    );
}

#[test]
fn cli_search_stale_schema_stderr_and_exit_code() {
    let (_tmp, root) = common::indexed_fixture();

    // Tamper with meta.json
    let meta_path = root.join(".ns").join("meta.json");
    let content = std::fs::read_to_string(&meta_path).expect("should read meta");
    let tampered = content.replace("\"schema_version\": 2", "\"schema_version\": 999");
    std::fs::write(&meta_path, &tampered).expect("should write tampered meta");

    let output = std::process::Command::new(ns_binary())
        .arg("EventStore")
        .current_dir(&root)
        .output()
        .expect("should run ns binary");

    assert!(!output.status.success(), "should exit with non-zero status");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("error: index was built with an older version of ns. Run 'ns index' to rebuild."),
        "stderr should have the exact schema mismatch message, got: {}",
        stderr
    );
}

#[test]
fn cli_no_results_exits_1_with_stderr_summary() {
    let (_tmp, root) = common::indexed_fixture();

    let output = std::process::Command::new(ns_binary())
        .arg("xyzzy_absolutely_no_match_ever_42")
        .current_dir(&root)
        .output()
        .expect("should run ns binary");

    assert!(!output.status.success(), "should exit 1 when no results");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("0 results"),
        "stderr should contain '0 results' summary, got: {}",
        stderr
    );
    // stdout should be empty (text mode skips stdout for zero results)
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.is_empty(),
        "stdout should be empty for zero-results text mode, got: {}",
        stdout
    );
}

#[test]
fn cli_no_results_json_exits_1_with_json_on_stdout() {
    let (_tmp, root) = common::indexed_fixture();

    let output = std::process::Command::new(ns_binary())
        .args(["--json", "xyzzy_absolutely_no_match_ever_42"])
        .current_dir(&root)
        .output()
        .expect("should run ns binary");

    assert!(!output.status.success(), "should exit 1 when no results");

    // stderr has the summary
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("0 results"),
        "stderr should contain '0 results', got: {}",
        stderr
    );

    // stdout has the JSON body
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON");
    assert_eq!(parsed["results"].as_array().unwrap().len(), 0);
    assert_eq!(parsed["stats"]["total_results"], 0);
}

#[test]
fn cli_search_success_exits_0() {
    let (_tmp, root) = common::indexed_fixture();

    let output = std::process::Command::new(ns_binary())
        .arg("EventStore")
        .current_dir(&root)
        .output()
        .expect("should run ns binary");

    assert!(output.status.success(), "should exit 0 when results found");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("event_store.rs"),
        "stdout should contain search results, got: {}",
        stdout
    );
}

#[test]
fn cli_no_results_files_only_exits_1_empty_stdout() {
    let (_tmp, root) = common::indexed_fixture();

    let output = std::process::Command::new(ns_binary())
        .args(["-l", "xyzzy_absolutely_no_match_ever_42"])
        .current_dir(&root)
        .output()
        .expect("should run ns binary");

    assert!(!output.status.success(), "should exit 1 when no results");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.is_empty(),
        "stdout should be empty for zero-results --files mode, got: {}",
        stdout
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("0 results"),
        "stderr should contain '0 results' summary, got: {}",
        stderr
    );
}

#[test]
fn cli_broken_pipe_does_not_panic() {
    let (_tmp, root) = common::indexed_fixture();

    // Pipe ns output to a process that immediately exits (closes stdin).
    // Before the fix, this caused a panic: "failed printing to stdout: Broken pipe".
    use std::process::{Command, Stdio};

    let mut child = Command::new(ns_binary())
        .arg("EventStore")
        .current_dir(&root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("should spawn ns");

    // Immediately drop stdout to simulate a broken pipe (consumer closed)
    drop(child.stdout.take());

    let result = child.wait_with_output().expect("should wait for ns");

    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        !stderr.contains("panicked"),
        "ns should not panic on broken pipe, stderr: {}",
        stderr
    );
}

// Note: The "index locked" error path (Issue 5) is not tested because reliably
// triggering a tantivy lock conflict in a test is fragile and race-prone.
// The lock detection itself is robust — it uses `TantivyError::LockFailure`
// variant matching (not string matching), so it will not regress silently
// across tantivy version upgrades.

// ── `ns search` subcommand tests ──────────────────────────────────────────────

#[test]
fn cli_search_subcommand_works() {
    let (_tmp, root) = common::indexed_fixture();

    let output = std::process::Command::new(ns_binary())
        .args(["search", "EventStore"])
        .current_dir(&root)
        .output()
        .expect("should run ns binary");

    assert!(
        output.status.success(),
        "ns search should exit 0 when results found"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("event_store.rs"),
        "ns search should return results, got: {}",
        stdout
    );
}

#[test]
fn cli_search_subcommand_disambiguates_index_query() {
    let (_tmp, root) = common::indexed_fixture();

    // "index" as a query would normally be parsed as the index subcommand.
    // `ns search "index"` should unambiguously search for the word "index".
    let output = std::process::Command::new(ns_binary())
        .args(["search", "index", "-l"])
        .current_dir(&root)
        .output()
        .expect("should run ns binary");

    // Should not get a subcommand parse error
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("unexpected argument"),
        "should not get subcommand parse error, got: {}",
        stderr
    );
    // Exit code 0 (found) or 1 (not found) are both acceptable — no crash/parse error
    assert!(
        output.status.code() == Some(0) || output.status.code() == Some(1),
        "should exit 0 or 1, got: {:?}",
        output.status.code()
    );
}

#[test]
fn cli_double_dash_disambiguates_query() {
    let (_tmp, root) = common::indexed_fixture();

    // `ns -- "index"` should also work as a search for "index"
    let output = std::process::Command::new(ns_binary())
        .args(["--", "index"])
        .current_dir(&root)
        .output()
        .expect("should run ns binary");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("unexpected argument"),
        "-- should disambiguate, got: {}",
        stderr
    );
    assert!(
        output.status.code() == Some(0) || output.status.code() == Some(1),
        "should exit 0 or 1, got: {:?}",
        output.status.code()
    );
}

#[test]
fn cli_search_subcommand_with_flags() {
    let (_tmp, root) = common::indexed_fixture();

    let output = std::process::Command::new(ns_binary())
        .args(["search", "EventStore", "--json", "-m", "5"])
        .current_dir(&root)
        .output()
        .expect("should run ns binary");

    assert!(output.status.success(), "ns search with flags should work");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("should be valid JSON");
    assert!(
        !parsed["results"].as_array().unwrap().is_empty(),
        "should have results"
    );
}

// ── Feature 5: Explainable ranking tests ──────────────────────────────────────

#[test]
fn json_output_has_ranking_factors() {
    let (_tmp, root) = common::indexed_fixture();

    let so =
        ns::searcher::search(&root, "EventStore", OutputMode::Json, &SearchOptions::default())
            .expect("search should work");

    let parsed: serde_json::Value = serde_json::from_str(&so.formatted).unwrap();

    let first = &parsed["results"][0];
    let rf = &first["ranking_factors"];
    assert!(rf.is_object(), "ranking_factors should be present as an object");
    assert!(rf["bm25_content"].is_number(), "bm25_content should be a number");
    assert!(rf["bm25_symbols"].is_number(), "bm25_symbols should be a number");
    assert_eq!(rf["symbol_boost"], "3x", "symbol_boost should be '3x'");
    assert!(rf["matched_fields"].is_array(), "matched_fields should be an array");

    // EventStore matches both content and symbols in event_store.rs
    let matched_fields: Vec<&str> = rf["matched_fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(
        matched_fields.contains(&"symbols"),
        "EventStore should match symbols field, got: {:?}",
        matched_fields
    );
}

#[test]
fn symbol_match_has_nonzero_bm25_symbols() {
    let (_tmp, root) = common::indexed_fixture();

    // "EventStore" is an extracted symbol in event_store.rs — should have nonzero bm25_symbols
    let (results, _stats) =
        ns::searcher::query::execute_search(&root, "EventStore", &opts(10))
            .expect("search should work");

    assert!(!results.is_empty());
    let event_store_result = results
        .iter()
        .find(|r| r.path.contains("event_store.rs"))
        .expect("event_store.rs should be in results");

    assert!(
        event_store_result.score_symbols > 0.0,
        "event_store.rs defines EventStore symbol, bm25_symbols should be > 0, got: {}",
        event_store_result.score_symbols
    );
    assert!(
        event_store_result.matched_fields.contains(&"symbols".to_string()),
        "matched_fields should contain 'symbols'"
    );
}

#[test]
fn content_only_match_has_zero_bm25_symbols() {
    let (_tmp, root) = common::indexed_fixture();

    // Search for a term that appears in file content but is not an extracted symbol name.
    // "pub" appears in Rust source content but is not a symbol name.
    let sym_opts = SearchOptions {
        max_results: 10,
        file_type: Some("rust".to_string()),
        ..Default::default()
    };
    let (results, _stats) =
        ns::searcher::query::execute_search(&root, "pub", &sym_opts)
            .expect("search should work");

    assert!(!results.is_empty(), "should find results for 'pub'");
    // At least one result should have zero bm25_symbols (pure content match)
    let has_content_only = results.iter().any(|r| r.score_symbols == 0.0);
    assert!(
        has_content_only,
        "at least one result for 'pub' should have zero bm25_symbols (content-only match), scores: {:?}",
        results.iter().map(|r| (r.path.as_str(), r.score_symbols)).collect::<Vec<_>>()
    );
}

// ── Performance smoke test ────────────────────────────────────────────────────

/// Indexes the ns source tree itself and verifies timing is reasonable.
///
/// This test is `#[ignore]`d by default — run with `cargo test -- --ignored`.
/// Requirement 7.4: index < 10s, search < 50ms on a medium repo.
#[test]
#[ignore]
fn performance_smoke_test() {
    // Copy repo source into a tempdir so .ns/ artifacts never pollute the real repo
    let tmp = tempfile::tempdir().expect("should create tempdir");
    let src = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let dest_src = tmp.path().join("src");
    copy_dir_recursive(&src, &dest_src);

    let root = tmp.path().to_path_buf();

    // Index
    let start = std::time::Instant::now();
    ns::indexer::run_full_index(&root, 1_048_576).expect("indexing should succeed");
    let index_ms = start.elapsed().as_millis();

    eprintln!("Performance: index took {}ms", index_ms);
    assert!(
        index_ms < 10_000,
        "indexing should complete in < 10s, took {}ms",
        index_ms
    );

    // Search
    let start = std::time::Instant::now();
    let (results, _stats) =
        ns::searcher::query::execute_search(&root, "EventStore", &opts(10))
            .expect("search should work");
    let search_ms = start.elapsed().as_millis();

    eprintln!("Performance: search took {}ms, found {} results", search_ms, results.len());
    assert!(
        search_ms < 50,
        "search should complete in < 50ms, took {}ms",
        search_ms
    );
    // tmp dir auto-cleaned on drop
}

/// Recursively copy a directory tree.
fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) {
    std::fs::create_dir_all(dst).expect("mkdir");
    for entry in std::fs::read_dir(src).expect("read_dir") {
        let entry = entry.expect("entry");
        let ty = entry.file_type().expect("file_type");
        let dest_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &dest_path);
        } else {
            std::fs::copy(entry.path(), &dest_path).expect("copy");
        }
    }
}

// ── Token efficiency: --max-context-lines and --budget tests ──────────────────

#[test]
fn max_context_lines_limits_output() {
    let (_tmp, root) = common::indexed_fixture();

    // With a small max_context_lines, the output should have fewer context lines
    let opts_small = SearchOptions {
        max_results: 1,
        max_context_lines: Some(3),
        ..Default::default()
    };
    let so_small =
        ns::searcher::search(&root, "EventStore", OutputMode::Text, &opts_small)
            .expect("search should work");

    // With unlimited max_context_lines (None), there should be at least as many
    let opts_unlimited = SearchOptions {
        max_results: 1,
        max_context_lines: None,
        ..Default::default()
    };
    let so_unlimited =
        ns::searcher::search(&root, "EventStore", OutputMode::Text, &opts_unlimited)
            .expect("search should work");

    // Count context lines (lines containing ":" with a line number pattern)
    let count_ctx = |s: &str| -> usize {
        s.lines()
            .filter(|l| {
                let trimmed = l.trim();
                // Context lines look like "   10: some code"
                trimmed.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)
            })
            .count()
    };
    let small_count = count_ctx(&so_small.formatted);
    let unlimited_count = count_ctx(&so_unlimited.formatted);

    assert!(
        small_count <= 3,
        "max_context_lines=3 should cap at 3 context lines, got {}",
        small_count
    );
    assert!(
        unlimited_count >= small_count,
        "unlimited should have at least as many lines as capped ({} vs {})",
        unlimited_count,
        small_count
    );
}

#[test]
fn max_context_lines_zero_means_unlimited() {
    let (_tmp, root) = common::indexed_fixture();

    let opts_zero = SearchOptions {
        max_results: 1,
        max_context_lines: Some(0),
        ..Default::default()
    };
    let opts_none = SearchOptions {
        max_results: 1,
        max_context_lines: None,
        ..Default::default()
    };

    let so_zero =
        ns::searcher::search(&root, "EventStore", OutputMode::Text, &opts_zero)
            .expect("search should work");
    let so_none =
        ns::searcher::search(&root, "EventStore", OutputMode::Text, &opts_none)
            .expect("search should work");

    assert_eq!(
        so_zero.formatted, so_none.formatted,
        "max_context_lines=Some(0) should produce the same output as None (unlimited)"
    );
}

#[test]
fn budget_limits_total_output_size() {
    let (_tmp, root) = common::indexed_fixture();

    // Very small budget should truncate results
    let opts_small = SearchOptions {
        max_results: 10,
        budget: Some(20), // 20 tokens = ~80 chars -- very tight
        ..Default::default()
    };
    let so_small =
        ns::searcher::search(&root, "fn", OutputMode::Text, &opts_small)
            .expect("search should work");

    // No budget should emit all results
    let opts_none = SearchOptions {
        max_results: 10,
        budget: None,
        ..Default::default()
    };
    let so_none =
        ns::searcher::search(&root, "fn", OutputMode::Text, &opts_none)
            .expect("search should work");

    // Budget-limited output should be shorter
    assert!(
        so_small.formatted.len() <= so_none.formatted.len(),
        "budget-limited output should not be longer than unlimited"
    );
    // If there were enough results to exhaust the budget, verify metadata
    if so_small.budget_exhausted {
        assert!(so_small.results_omitted > 0);
        assert!(so_small.formatted.contains("budget exceeded"));
    }
}

#[test]
fn budget_zero_means_unlimited() {
    let (_tmp, root) = common::indexed_fixture();

    // budget=0 should be treated as unlimited at the CLI layer.
    // At the library level, budget=None is unlimited, and the CLI converts 0 to None.
    // So test that budget=None produces the same output as unlimited.
    let opts = SearchOptions {
        max_results: 10,
        budget: None,
        ..Default::default()
    };
    let so =
        ns::searcher::search(&root, "EventStore", OutputMode::Text, &opts)
            .expect("search should work");

    assert!(!so.budget_exhausted, "unlimited budget should not be exhausted");
    assert_eq!(so.results_omitted, 0);
}

#[test]
fn cli_budget_zero_is_unlimited() {
    let (_tmp, root) = common::indexed_fixture();

    let output = std::process::Command::new(ns_binary())
        .args(["--budget", "0", "EventStore"])
        .current_dir(&root)
        .output()
        .expect("should run ns binary");

    assert!(output.status.success(), "budget=0 should succeed");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("budget exceeded"),
        "budget=0 should be unlimited, got stderr: {}",
        stderr
    );
}

#[test]
fn budget_exceeded_shows_in_json_output() {
    let (_tmp, root) = common::indexed_fixture();

    // Very tight budget in JSON mode
    let opts = SearchOptions {
        max_results: 10,
        budget: Some(20), // Very tight
        ..Default::default()
    };
    let so =
        ns::searcher::search(&root, "fn", OutputMode::Json, &opts)
            .expect("search should work");

    let parsed: serde_json::Value = serde_json::from_str(&so.formatted)
        .expect("should be valid JSON");

    if so.budget_exhausted {
        assert_eq!(
            parsed["stats"]["budget_exceeded"], true,
            "JSON stats should contain budget_exceeded: true"
        );
        assert!(
            parsed["stats"]["results_omitted"].as_u64().unwrap() > 0,
            "JSON stats should contain results_omitted > 0"
        );
    }
    // Even with budget, output should be valid JSON with results array
    assert!(parsed["results"].is_array());
}

#[test]
fn cli_max_context_lines_flag_works() {
    let (_tmp, root) = common::indexed_fixture();

    let output = std::process::Command::new(ns_binary())
        .args(["--max-context-lines", "2", "EventStore"])
        .current_dir(&root)
        .output()
        .expect("should run ns binary");

    assert!(output.status.success(), "should succeed with --max-context-lines");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("event_store.rs"),
        "should find event_store.rs"
    );
}

#[test]
fn cli_budget_flag_works() {
    let (_tmp, root) = common::indexed_fixture();

    let output = std::process::Command::new(ns_binary())
        .args(["--budget", "500", "EventStore"])
        .current_dir(&root)
        .output()
        .expect("should run ns binary");

    assert!(output.status.success(), "should succeed with --budget");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("event_store.rs"),
        "should find event_store.rs"
    );
}
