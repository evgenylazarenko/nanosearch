pub mod hooks;
pub mod index;
pub mod search;
pub mod status;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "ns",
    about = "Nano Search -- ranked code search for LLM agents",
    version
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Search query (default when no subcommand is given)
    pub query: Option<String>,

    /// Language filter (e.g. rust, python, go)
    #[arg(short = 't', long = "type", global = true)]
    pub file_type: Option<String>,

    /// Path glob filter
    #[arg(short = 'g', long = "glob", global = true)]
    pub file_glob: Option<String>,

    /// Show matching file paths only
    #[arg(short = 'l', long = "files")]
    pub files_only: bool,

    /// Case-insensitive search (accepted for rg compatibility; always on)
    #[arg(short = 'i', long = "ignore-case")]
    pub ignore_case: bool,

    /// Maximum number of results
    #[arg(short = 'm', long = "max-count", default_value_t = 10)]
    pub max_count: usize,

    /// Context lines around matches
    #[arg(short = 'C', long = "context", default_value_t = 1)]
    pub context: usize,

    /// Output results as JSON
    #[arg(long = "json")]
    pub json: bool,

    /// Symbol-only search
    #[arg(long = "sym")]
    pub sym: bool,

    /// Fuzzy search
    #[arg(long = "fuzzy")]
    pub fuzzy: bool,

    /// Max context lines per file (0 = unlimited)
    #[arg(long = "max-context-lines", default_value_t = 30)]
    pub max_context_lines: usize,

    /// Token budget for total output (approximate)
    #[arg(long = "budget")]
    pub budget: Option<usize>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Search the index (use when query matches a subcommand name)
    Search(SearchSubArgs),
    /// Build or update the search index
    Index(IndexArgs),
    /// Show index status
    Status,
    /// Manage git hooks
    Hooks {
        #[command(subcommand)]
        action: HooksAction,
    },
}

#[derive(Parser)]
pub struct SearchSubArgs {
    /// Search query
    pub query: String,

    /// Show matching file paths only
    #[arg(short = 'l', long = "files")]
    pub files_only: bool,

    /// Case-insensitive search (accepted for rg compatibility; always on)
    #[arg(short = 'i', long = "ignore-case")]
    pub ignore_case: bool,

    /// Maximum number of results
    #[arg(short = 'm', long = "max-count", default_value_t = 10)]
    pub max_count: usize,

    /// Context lines around matches
    #[arg(short = 'C', long = "context", default_value_t = 1)]
    pub context: usize,

    /// Output results as JSON
    #[arg(long = "json")]
    pub json: bool,

    /// Symbol-only search
    #[arg(long = "sym")]
    pub sym: bool,

    /// Fuzzy search
    #[arg(long = "fuzzy")]
    pub fuzzy: bool,

    /// Max context lines per file (0 = unlimited)
    #[arg(long = "max-context-lines", default_value_t = 30)]
    pub max_context_lines: usize,

    /// Token budget for total output (approximate)
    #[arg(long = "budget")]
    pub budget: Option<usize>,
}

#[derive(Parser)]
pub struct IndexArgs {
    /// Incremental indexing (only changed files)
    #[arg(long)]
    pub incremental: bool,

    /// Repository root directory
    #[arg(long = "root")]
    pub root: Option<PathBuf>,

    /// Maximum file size in bytes (default: 1 MB)
    #[arg(long = "max-file-size", default_value_t = 1_048_576)]
    pub max_file_size: u64,
}

#[derive(Subcommand)]
pub enum HooksAction {
    /// Install git hooks for automatic re-indexing
    Install,
    /// Remove installed git hooks
    Remove,
}

/// Extracts search args from the top-level Cli struct.
pub struct SearchArgs {
    pub query: String,
    pub file_type: Option<String>,
    pub file_glob: Option<String>,
    pub files_only: bool,
    pub max_count: usize,
    pub context: usize,
    pub json: bool,
    pub sym: bool,
    pub fuzzy: bool,
    pub max_context_lines: usize,
    pub budget: Option<usize>,
}

impl SearchArgs {
    pub fn from_cli(cli: &Cli, query: String) -> Self {
        Self {
            query,
            file_type: cli.file_type.clone(),
            file_glob: cli.file_glob.clone(),
            files_only: cli.files_only,
            max_count: cli.max_count,
            context: cli.context,
            json: cli.json,
            sym: cli.sym,
            fuzzy: cli.fuzzy,
            max_context_lines: cli.max_context_lines,
            budget: cli.budget,
        }
    }

    pub fn from_search_sub(sub: &SearchSubArgs, cli: &Cli) -> Self {
        Self {
            query: sub.query.clone(),
            file_type: cli.file_type.clone(),
            file_glob: cli.file_glob.clone(),
            files_only: sub.files_only,
            max_count: sub.max_count,
            context: sub.context,
            json: sub.json,
            sym: sub.sym,
            fuzzy: sub.fuzzy,
            max_context_lines: sub.max_context_lines,
            budget: sub.budget,
        }
    }
}
