# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
# Build
cargo build --release

# Run tests
cargo test

# Run a single test
cargo test <test_name>

# Run integration tests only
cargo test --test integration_search
cargo test --test integration_index
cargo test --test integration_incremental
cargo test --test integration_hooks

# Build and run quickly
cargo run -- -- "query"
```

## Code Search Policy

**Primary tool: `ns`** (this repo's own binary — must be built first)

```bash
# Files only (default starting point — avoids token-heavy output)
ns -l -m 20 -- "{query}"
ns -l -m 20 --sym -- "{query}"

# Contextual output (after shortlist identified)
ns --budget 500 --max-context-lines 8 -C 1 -m 5 -- "{query}"
```

Retry ladder on miss: files → sym → fuzzy → type/glob filter → `rg -l`.
Run `ns index --incremental` after meaningful edits or branch switches.
Use `rg` only for literal/regex patterns, exact string checks, or after ns retry ladder fails.

## Architecture

**Binary:** `src/main.rs` — CLI entry point, dispatches to subcommands.

**Modules (private, binary-only):**
- `src/cmd/` — CLI argument parsing (`clap`) and subcommand dispatch: `search`, `index`, `status`, `hooks`.
- `src/schema.rs` — Tantivy schema (5 fields: `content`, `symbols`, `symbols_raw`, `path`, `lang`). Bump `SCHEMA_VERSION` in `writer.rs` when changing schema.
- `src/indexer/` — Full and incremental indexing pipeline:
  - `walker.rs` — `.gitignore`-aware file walker using the `ignore` crate.
  - `language.rs` — Extension-to-language mapping.
  - `symbols.rs` — Tree-sitter symbol extraction (Rust, TS, JS, Python, Go, Elixir).
  - `writer.rs` — Builds/opens the Tantivy index; writes `meta.json` with `SCHEMA_VERSION`.
  - `incremental.rs` — Git diff or mtime-based change detection for incremental re-indexing.
- `src/searcher/` — Search pipeline:
  - `query.rs` — Tantivy query execution. Default search boosts `symbols` 3× over `content`. `--sym` searches symbols only. `--fuzzy` uses `FuzzyTermQuery` (Levenshtein distance 1). Language filter uses a `TermQuery` on `lang`. Glob filter is post-search.
  - `context.rs` — Extracts context lines from files for result display.
  - `format.rs` — Formats results as text, files-only, or JSON.
- `src/stats.rs` — Per-search stats tracking (`stats.json`) and append-only search log (`search_log.jsonl`). Both files live in `.ns/`. File locking (`fs4`) ensures concurrent safety.
- `src/error.rs` — `NsError` enum covering IO, Tantivy, query parse, JSON, schema mismatch, and glob errors.

**Index storage:** `.ns/index/` (Tantivy), `.ns/meta.json` (schema version, file count, git commit), `.ns/stats.json` (cumulative search stats), `.ns/search_log.jsonl` (per-invocation log).

**Public library surface (`src/lib.rs`):** exposes `error`, `indexer`, `schema`, `searcher`, `stats` — used by integration tests in `tests/`.

**Tests:** `tests/` contains integration tests using `tempfile` and the fixture repo at `tests/fixtures/sample_repo`. Unit tests live inline in each source file.

## Key Design Decisions

- **File-level index, not line-level:** Each file is one Tantivy document. Results are ranked files, not scattered lines.
- **Symbols stored twice:** `symbols` (tokenized, not stored) for BM25 search; `symbols_raw` (pipe-delimited, stored) for display. Never search `symbols_raw` directly.
- **Schema version guard:** `open_index` reads `meta.json` and rejects mismatches — triggers `ns index` rebuild. Bump `SCHEMA_VERSION` (in `writer.rs`) on any schema change.
- **Token budget:** `--budget N` caps output at ~N tokens (1 token ≈ 4 chars) to protect agent context windows. Implemented in the searcher, not the formatter.
- **No runtime dependencies:** Single binary, no server, no external services.
