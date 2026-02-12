use std::path::Path;
use std::time::Instant;

use tantivy::collector::TopDocs;
use tantivy::query::{
    BooleanQuery, BoostQuery, FuzzyTermQuery, Occur, Query, QueryParser, TermQuery,
};
use tantivy::schema::{IndexRecordOption, Value};
use tantivy::{ReloadPolicy, TantivyDocument, Term};

use crate::error::NsError;
use crate::indexer::writer::open_index;
use crate::schema::{content_field, lang_field, path_field, symbols_field, symbols_raw_field};

/// A single search result from the tantivy index.
#[derive(Debug)]
pub struct SearchResult {
    /// File path relative to the repo root.
    pub path: String,
    /// BM25 relevance score.
    pub score: f32,
    /// Detected language, or None if unknown.
    pub lang: Option<String>,
    /// Raw symbol names extracted from the document (pipe-separated in index).
    pub symbols_raw: Vec<String>,
}

/// Summary statistics for a search operation.
#[derive(Debug)]
pub struct SearchStats {
    /// Number of results returned.
    pub total_results: usize,
    /// Total files in the index.
    pub files_searched: usize,
    /// Time taken for the search in milliseconds.
    pub elapsed_ms: u64,
}

/// Options that control search behaviour — maps 1:1 to CLI flags.
#[derive(Debug)]
pub struct SearchOptions {
    /// Maximum number of results.
    pub max_results: usize,
    /// Context lines around matches (±N).
    pub context_window: usize,
    /// Language filter (e.g. "rust", "python").
    pub file_type: Option<String>,
    /// Glob pattern to filter file paths (e.g. "src/*").
    pub file_glob: Option<String>,
    /// Search only symbol names, not file content.
    pub sym_only: bool,
    /// Use fuzzy matching (Levenshtein distance 1).
    pub fuzzy: bool,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            max_results: 10,
            context_window: 1,
            file_type: None,
            file_glob: None,
            sym_only: false,
            fuzzy: false,
        }
    }
}

/// Maximum number of results to prevent unbounded file I/O during context extraction.
const MAX_RESULTS_CEILING: usize = 100;

/// Executes a search query against the index at `root`.
///
/// Opens the index (reads `meta.json` once), executes the BM25 query,
/// and returns ranked results plus stats.
/// `max_results` is clamped to `MAX_RESULTS_CEILING` (100) to prevent
/// unbounded disk I/O during context extraction.
///
/// Search modes:
/// - Default: searches both `content` and `symbols` fields, 3x boost on `symbols`.
/// - `sym_only`: searches only `symbols` field (no content).
/// - `fuzzy`: builds per-term `FuzzyTermQuery` (Levenshtein distance 1) instead
///   of using the `QueryParser`, with `Should` occurrence so any term can match.
///
/// Filters:
/// - `file_type`: restricts results to files with the given language via a
///   `TermQuery` on the `lang` field combined with `BooleanQuery`.
/// - `file_glob`: post-filters results by matching `path` against a glob pattern.
pub fn execute_search(
    root: &Path,
    query_str: &str,
    opts: &SearchOptions,
) -> Result<(Vec<SearchResult>, SearchStats), NsError> {
    let max_results = opts.max_results.min(MAX_RESULTS_CEILING);
    let (index, meta) = open_index(root)?;

    let schema = index.schema();
    let content = content_field(&schema);
    let symbols_f = symbols_field(&schema);
    let path_f = path_field(&schema);
    let lang_f = lang_field(&schema);
    let symbols_raw_f = symbols_raw_field(&schema);

    // Build the base query based on mode
    let base_query: Box<dyn Query> = if opts.fuzzy {
        build_fuzzy_query(query_str, content, symbols_f, opts.sym_only)
    } else if opts.sym_only {
        let parser = QueryParser::for_index(&index, vec![symbols_f]);
        parser.parse_query(query_str)?
    } else {
        let mut parser = QueryParser::for_index(&index, vec![content, symbols_f]);
        parser.set_field_boost(symbols_f, 3.0);
        parser.parse_query(query_str)?
    };

    // Wrap with language filter if specified
    let query: Box<dyn Query> = if let Some(ref lang_filter) = opts.file_type {
        let lang_query: Box<dyn Query> = Box::new(TermQuery::new(
            Term::from_field_text(lang_f, lang_filter),
            IndexRecordOption::Basic,
        ));
        Box::new(BooleanQuery::new(vec![
            (Occur::Must, base_query),
            (Occur::Must, lang_query),
        ]))
    } else {
        base_query
    };

    let reader = index
        .reader_builder()
        .reload_policy(ReloadPolicy::Manual)
        .try_into()?;
    let searcher = reader.searcher();

    let start = Instant::now();
    let top_docs = searcher.search(&query, &TopDocs::with_limit(max_results))?;
    let elapsed_ms = start.elapsed().as_millis() as u64;

    let mut results = Vec::with_capacity(top_docs.len());
    for (score, doc_address) in &top_docs {
        let doc: TantivyDocument = searcher.doc(*doc_address)?;

        let path_val = doc
            .get_first(path_f)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let lang_val = doc
            .get_first(lang_f)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty());

        let symbols_raw_val = doc
            .get_first(symbols_raw_f)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let symbols: Vec<String> = if symbols_raw_val.is_empty() {
            Vec::new()
        } else {
            symbols_raw_val.split('|').map(|s| s.to_string()).collect()
        };

        results.push(SearchResult {
            path: path_val,
            score: *score,
            lang: lang_val,
            symbols_raw: symbols,
        });
    }

    // Post-filter by glob pattern if specified
    if let Some(ref glob_pattern) = opts.file_glob {
        let pattern = glob::Pattern::new(glob_pattern)?;
        results.retain(|r| pattern.matches(&r.path));
    }

    let stats = SearchStats {
        total_results: results.len(),
        files_searched: meta.file_count,
        elapsed_ms,
    };

    Ok((results, stats))
}

/// Builds a fuzzy query by tokenizing the input, creating a `FuzzyTermQuery`
/// per token (Levenshtein distance 1, transposition cost 1), and combining
/// them with `Should` occurrence so any term match contributes.
///
/// If `sym_only` is false, each term generates two clauses: one for `content`
/// and one for `symbols` (with 3x boost on symbols).
fn build_fuzzy_query(
    query_str: &str,
    content_field: tantivy::schema::Field,
    symbols_field: tantivy::schema::Field,
    sym_only: bool,
) -> Box<dyn Query> {
    let terms: Vec<&str> = query_str
        .split_whitespace()
        .filter(|s| !s.is_empty())
        .collect();

    let mut clauses: Vec<(Occur, Box<dyn Query>)> = Vec::new();

    for term_str in &terms {
        let lower = term_str.to_lowercase();

        if sym_only {
            // Only symbols field
            let ft = FuzzyTermQuery::new(
                Term::from_field_text(symbols_field, &lower),
                1,
                true,
            );
            clauses.push((Occur::Should, Box::new(ft)));
        } else {
            // Content field (no boost)
            let ft_content = FuzzyTermQuery::new(
                Term::from_field_text(content_field, &lower),
                1,
                true,
            );
            clauses.push((Occur::Should, Box::new(ft_content)));

            // Symbols field with 3x boost
            let ft_symbols = FuzzyTermQuery::new(
                Term::from_field_text(symbols_field, &lower),
                1,
                true,
            );
            let boosted: Box<dyn Query> = Box::new(BoostQuery::new(Box::new(ft_symbols), 3.0));
            clauses.push((Occur::Should, boosted));
        }
    }

    if clauses.is_empty() {
        // Empty query — return an all-docs query that matches nothing
        Box::new(BooleanQuery::new(vec![]))
    } else {
        Box::new(BooleanQuery::new(clauses))
    }
}
