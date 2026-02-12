mod common;

#[test]
fn search_returns_ranked_results() {
    let (_tmp, root) = common::indexed_fixture();

    let (results, stats) =
        ns::searcher::query::execute_search(&root, "EventStore", 10).expect("search should work");

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
        ns::searcher::search(&root, "EventStore", 10, 1).expect("search pipeline should work");

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
        ns::searcher::query::execute_search(&root, "validate port", 10).expect("search should work");

    assert!(!results.is_empty(), "should find results for multi-term query");
    let has_validator = results.iter().any(|r| r.path.contains("validator.rs"));
    assert!(has_validator, "validator.rs should appear in results");
}

#[test]
fn search_no_results_returns_empty() {
    let (_tmp, root) = common::indexed_fixture();

    let (results, stats) =
        ns::searcher::query::execute_search(&root, "xyzzy_nonexistent_term", 10)
            .expect("search should succeed even with no matches");

    assert!(results.is_empty(), "should find no results for nonsense term");
    assert_eq!(stats.total_results, 0);
}

#[test]
fn search_respects_max_results() {
    let (_tmp, root) = common::indexed_fixture();

    let (results, _stats) =
        ns::searcher::query::execute_search(&root, "fn", 2).expect("search should work");

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
        ns::searcher::search(&root, "EventStore", 5, 1).expect("search should work");

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
        ns::searcher::search(&root, "EventStore", 5, 0).expect("search should work");

    // With context=0 and multiple match locations, there should be group separators
    assert!(
        output.contains("..."),
        "non-contiguous context groups should be separated by '...'"
    );
}
