use std::path::Path;
use std::time::Instant;

use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::Value;
use tantivy::{ReloadPolicy, TantivyDocument};

use crate::error::NsError;
use crate::indexer::writer::open_index;
use crate::schema::{content_field, lang_field, path_field};

/// A single search result from the tantivy index.
pub struct SearchResult {
    /// File path relative to the repo root.
    pub path: String,
    /// BM25 relevance score.
    pub score: f32,
    /// Detected language, or None if unknown.
    pub lang: Option<String>,
}

/// Summary statistics for a search operation.
pub struct SearchStats {
    /// Number of results returned.
    pub total_results: usize,
    /// Total files in the index.
    pub files_searched: usize,
    /// Time taken for the search in milliseconds.
    pub elapsed_ms: u64,
}

/// Maximum number of results to prevent unbounded file I/O during context extraction.
const MAX_RESULTS_CEILING: usize = 100;

/// Executes a search query against the index at `root`.
///
/// Opens the index (reads `meta.json` once), executes the BM25 query,
/// and returns ranked results plus stats.
/// `max_results` is clamped to `MAX_RESULTS_CEILING` (100) to prevent
/// unbounded disk I/O during context extraction.
/// Currently searches only the `content` field (symbol boost comes in Phase 4).
pub fn execute_search(
    root: &Path,
    query_str: &str,
    max_results: usize,
) -> Result<(Vec<SearchResult>, SearchStats), NsError> {
    let max_results = max_results.min(MAX_RESULTS_CEILING);
    let (index, meta) = open_index(root)?;

    let schema = index.schema();
    let content = content_field(&schema);
    let path_f = path_field(&schema);
    let lang_f = lang_field(&schema);

    // Build query parser for content field only (Phase 4 adds symbols with boost)
    let query_parser = QueryParser::for_index(&index, vec![content]);
    let query = query_parser.parse_query(query_str)?;

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

        results.push(SearchResult {
            path: path_val,
            score: *score,
            lang: lang_val,
        });
    }

    let stats = SearchStats {
        total_results: results.len(),
        files_searched: meta.file_count,
        elapsed_ms,
    };

    Ok((results, stats))
}
