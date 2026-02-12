use std::fs;
use std::path::Path;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tantivy::tokenizer::{LowerCaser, TextAnalyzer, WhitespaceTokenizer};
use tantivy::{doc, Index, IndexWriter};

use crate::schema::{
    build_schema, content_field, lang_field, path_field, symbols_field, symbols_raw_field,
};

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
pub const SCHEMA_VERSION: u32 = 1;

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
/// Returns the number of files indexed.
pub fn build_index(root: &Path, files: &[WalkedFile]) -> Result<usize, Box<dyn std::error::Error>> {
    let ns_dir = root.join(".ns");
    let index_dir = ns_dir.join("index");
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
        writer.add_document(doc!(
            content => file.content.as_str(),
            symbols => "",
            symbols_raw => "",
            path => file.rel_path.as_str(),
            lang => file.lang.as_str(),
        ))?;
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
        indexed_at: chrono_now(),
        git_commit,
        file_count,
        index_size_bytes: index_size,
    };

    let meta_path = ns_dir.join("meta.json");
    let meta_json = serde_json::to_string_pretty(&meta)?;
    fs::write(&meta_path, &meta_json)?;

    eprintln!(
        "Indexed {} files in {}ms",
        file_count,
        elapsed.as_millis()
    );

    // Check if .ns/ is in .gitignore
    check_gitignore_warning(root);

    Ok(file_count)
}

/// Opens an existing index at `.ns/index/` for reading or incremental writes.
pub fn open_index(root: &Path) -> Result<Index, Box<dyn std::error::Error>> {
    let index_dir = root.join(".ns").join("index");
    let schema = build_schema();
    let index = Index::open_in_dir(&index_dir)?;

    // Verify schema matches
    if index.schema() != schema {
        return Err("index schema mismatch — run `ns index` to rebuild".into());
    }

    register_symbol_tokenizer(&index);
    Ok(index)
}

/// Reads `.ns/meta.json`.
pub fn read_meta(root: &Path) -> Result<IndexMeta, Box<dyn std::error::Error>> {
    let meta_path = root.join(".ns").join("meta.json");
    let content = fs::read_to_string(&meta_path)?;
    let meta: IndexMeta = serde_json::from_str(&content)?;
    Ok(meta)
}

fn dir_size(path: &Path) -> u64 {
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

fn get_git_commit(root: &Path) -> Option<String> {
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

fn chrono_now() -> String {
    // Simple ISO 8601 timestamp without pulling in chrono crate
    let output = std::process::Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .output();
    match output {
        Ok(o) if o.status.success() => {
            String::from_utf8_lossy(&o.stdout).trim().to_string()
        }
        _ => "unknown".to_string(),
    }
}

fn check_gitignore_warning(root: &Path) {
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
