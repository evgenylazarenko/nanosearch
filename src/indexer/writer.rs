use std::fs;
use std::path::Path;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tantivy::tokenizer::{LowerCaser, TextAnalyzer, WhitespaceTokenizer};
use tantivy::{Index, IndexWriter, TantivyDocument};

use crate::error::NsError;
use crate::schema::{
    build_schema, content_field, lang_field, path_field, symbols_field, symbols_raw_field,
};

use super::symbols::extract_symbols;
use super::walker::WalkedFile;

/// Metadata written to `.ns/meta.json` after indexing.
#[derive(Serialize, Deserialize, Debug)]
pub struct IndexMeta {
    pub schema_version: u32,
    pub indexed_at: String,
    pub git_commit: Option<String>,
    pub file_count: usize,
    pub index_size_bytes: u64,
}

/// Current schema version. Bump when schema changes.
pub const SCHEMA_VERSION: u32 = 2;

/// Stats returned by a full index build.
#[derive(Debug)]
pub struct FullIndexStats {
    pub file_count: usize,
    pub elapsed_ms: u64,
}

/// Registers the custom "symbol" tokenizer on a tantivy index.
pub fn register_symbol_tokenizer(index: &Index) {
    let tokenizer = TextAnalyzer::builder(WhitespaceTokenizer::default())
        .filter(LowerCaser)
        .build();
    index.tokenizers().register("symbol", tokenizer);
}

/// Builds the tantivy index from walked files.
///
/// Creates `.ns/index/` directory, writes documents, commits, and writes `meta.json`.
/// Returns index stats (file count, elapsed time). Does not print to stderr.
pub fn build_index(root: &Path, files: &[WalkedFile]) -> Result<FullIndexStats, NsError> {
    let ns_dir = root.join(".ns");
    let index_dir = ns_dir.join("index");

    // Wipe existing index for a clean full rebuild.
    // create_in_dir requires an empty (or non-existent) directory.
    if index_dir.exists() {
        fs::remove_dir_all(&index_dir)?;
    }
    fs::create_dir_all(&index_dir)?;

    let schema = build_schema();
    let index = Index::create_in_dir(&index_dir, schema.clone())?;
    register_symbol_tokenizer(&index);

    let content = content_field(&schema);
    let symbols = symbols_field(&schema);
    let symbols_raw = symbols_raw_field(&schema);
    let path = path_field(&schema);
    let lang = lang_field(&schema);

    // 50 MB heap for the writer
    let mut writer: IndexWriter = index.writer(50_000_000)?;

    let start = Instant::now();

    for file in files {
        let mut doc = TantivyDocument::new();
        doc.add_text(content, &file.content);

        // Extract symbols via tree-sitter for supported languages
        let symbol_names = file
            .lang
            .as_deref()
            .map(|l| extract_symbols(l, file.content.as_bytes()))
            .unwrap_or_default();

        // symbols: space-separated for tokenized search
        doc.add_text(symbols, &symbol_names.join(" "));
        // symbols_raw: pipe-separated, original casing, for display
        doc.add_text(symbols_raw, &symbol_names.join("|"));

        doc.add_text(path, &file.rel_path);
        if let Some(ref lang_str) = file.lang {
            doc.add_text(lang, lang_str);
        }
        writer.add_document(doc)?;
    }

    writer.commit()?;

    let elapsed = start.elapsed();
    let file_count = files.len();

    // Calculate index size
    let index_size = dir_size(&index_dir);

    // Get current git commit
    let git_commit = get_git_commit(root);

    // Write meta.json
    let meta = IndexMeta {
        schema_version: SCHEMA_VERSION,
        indexed_at: utc_timestamp_iso8601(),
        git_commit,
        file_count,
        index_size_bytes: index_size,
    };

    let meta_path = ns_dir.join("meta.json");
    let meta_json = serde_json::to_string(&meta)?;
    fs::write(&meta_path, &meta_json)?;

    Ok(FullIndexStats {
        file_count,
        elapsed_ms: elapsed.as_millis() as u64,
    })
}

/// Opens an existing index at `.ns/index/` for reading or incremental writes.
///
/// Reads `meta.json` once and returns it alongside the index, so callers
/// don't need to re-read it for file_count or other metadata.
///
/// Validates `SCHEMA_VERSION` from `meta.json` rather than comparing tantivy `Schema`
/// objects directly — the latter is fragile across tantivy upgrades where default
/// options may drift.
pub fn open_index(root: &Path) -> Result<(Index, IndexMeta), NsError> {
    let meta = read_meta(root)?;
    if meta.schema_version != SCHEMA_VERSION {
        return Err(NsError::SchemaVersionMismatch {
            found: meta.schema_version,
            expected: SCHEMA_VERSION,
        });
    }

    let index_dir = root.join(".ns").join("index");
    let index = Index::open_in_dir(&index_dir)?;

    register_symbol_tokenizer(&index);
    Ok((index, meta))
}

/// Reads `.ns/meta.json`.
pub fn read_meta(root: &Path) -> Result<IndexMeta, NsError> {
    let meta_path = root.join(".ns").join("meta.json");
    let content = fs::read_to_string(&meta_path)?;
    let meta: IndexMeta = serde_json::from_str(&content)?;
    Ok(meta)
}

pub(crate) fn dir_size(path: &Path) -> u64 {
    let mut size = 0;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let meta = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            if meta.is_file() {
                size += meta.len();
            } else if meta.is_dir() {
                size += dir_size(&entry.path());
            }
        }
    }
    size
}

pub(crate) fn get_git_commit(root: &Path) -> Option<String> {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
            } else {
                None
            }
        })
}

pub(crate) fn utc_timestamp_iso8601() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Manual UTC breakdown — avoids pulling in chrono/time crate
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Days since 1970-01-01 → (year, month, day) via civil calendar algorithm
    // Ref: http://howardhinnant.github.io/date_algorithms.html
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, m, d, hours, minutes, seconds)
}

pub fn check_gitignore_warning(root: &Path) {
    // Only warn in git repositories — non-git dirs have no .gitignore to update
    if !root.join(".git").exists() {
        return;
    }
    let gitignore_path = root.join(".gitignore");
    if gitignore_path.exists() {
        if let Ok(content) = fs::read_to_string(&gitignore_path) {
            if content.lines().any(|line| {
                let trimmed = line.trim();
                trimmed == ".ns/" || trimmed == ".ns" || trimmed == "/.ns/" || trimmed == "/.ns"
            }) {
                return; // .ns/ is already in .gitignore
            }
        }
    }
    eprintln!("warning: .ns/ is not in .gitignore. Add it to avoid committing the index.");
}
