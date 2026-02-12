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

    let (output, stats) =
        ns::searcher::search(&root, "EventStore", OutputMode::Text, &SearchOptions::default())
            .expect("search pipeline should work");

    assert!(output.contains("[1]"), "should have result rank [1]");
    assert!(output.contains("event_store.rs"), "should show event_store.rs");
    assert!(output.contains("score:"), "should show score");
    assert!(output.contains("lang: rust"), "should show lang");
    assert!(
        output.contains("result (searched"),
        "should show summary line"
    );
    assert!(stats.total_results > 0);
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

    let (output, _stats) =
        ns::searcher::search(
            &root,
            "EventStore",
            OutputMode::Text,
            &SearchOptions { max_results: 5, ..Default::default() },
        )
        .expect("search should work");

    assert!(
        output.contains("pub struct EventStore"),
        "output should contain context line with struct definition"
    );
}

#[test]
fn search_context_shows_separators_between_groups() {
    let (_tmp, root) = common::indexed_fixture();

    // EventStore appears on multiple non-contiguous lines in event_store.rs
    let (output, _stats) =
        ns::searcher::search(
            &root,
            "EventStore",
            OutputMode::Text,
            &SearchOptions { max_results: 5, context_window: 0, ..Default::default() },
        )
        .expect("search should work");

    // With context=0 and multiple match locations, there should be group separators
    assert!(
        output.contains("..."),
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

    let (output, stats) =
        ns::searcher::search(&root, "EventStore", OutputMode::FilesOnly, &SearchOptions::default())
            .expect("search should work");

    assert!(stats.total_results > 0);
    // Output should be bare paths — no scores, no context, no brackets
    assert!(!output.contains("[1]"), "files-only should not have rank markers");
    assert!(!output.contains("score:"), "files-only should not have scores");
    // Each line should be a file path
    for line in output.lines() {
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

    let (output, stats) =
        ns::searcher::search(&root, "EventStore", OutputMode::Json, &SearchOptions::default())
            .expect("search should work");

    assert!(stats.total_results > 0);
    let parsed: serde_json::Value =
        serde_json::from_str(&output).expect("output should be valid JSON");

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

    let (output, _stats) =
        ns::searcher::search(&root, "EventStore", OutputMode::Json, &SearchOptions::default())
            .expect("search should work");

    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

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

    let (output, _stats) =
        ns::searcher::search(&root, "EventStore", OutputMode::Json, &SearchOptions::default())
            .expect("search should work");

    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
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
