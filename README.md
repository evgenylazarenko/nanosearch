# ns — Nano Search

Ranked code search for LLM agents. Single binary, no server, no query language.

```
$ ns -- "EventStore"

 [1] src/services/event_store.rs        (score: 12.4, lang: rust)
     42: pub struct EventStore {
     43:     db: DatabasePool,
     58: impl EventStore {
     59:     pub fn new(db: DatabasePool) -> Self {

 [2] src/services/reconciliation.rs     (score: 8.1, lang: rust)
     12: use crate::services::event_store::EventStore;

2 results (searched 847 files in 2ms)
```

## Why

LLM coding agents need to search codebases. The existing options all have trade-offs:

- **grep/ripgrep** — Fast but unranked. Every match comes back with no signal about relevance. Searching `"handle"` in a real repo returns hundreds of hits with no way to know which file matters.
- **Zoekt/Sourcegraph** — Excellent ranked search but requires a server process, a Go runtime, and a query language (`repo:`, `file:`, `sym:` syntax) that LLMs hallucinate.
- **Embeddings-based search** — Adds model dependencies, vector databases, and latency. Vibes-based retrieval is the wrong tool for code.

ns fills the gap: **better than grep, simpler than Sourcegraph.** BM25 ranking means results come back in relevance order. Symbol awareness means function/class definitions rank higher than random mentions in comments. The CLI uses flags (not a query language) that mirror ripgrep — LLMs already know how to use it.

## Install

```
cargo build --release
cp target/release/ns /usr/local/bin/
```

One binary. No runtime dependencies.

## Quick start

```bash
# Index your repo (run from repo root)
ns index

# Search
ns -- "UserRepository"

# Keep index fresh automatically
ns hooks install
```

## How it works

ns builds a local search index in `.ns/` at your repo root. Files are indexed with [tantivy](https://github.com/quickwit-oss/tantivy) (BM25 scoring) and parsed with [tree-sitter](https://tree-sitter.github.io/) to extract symbol names. When you search, results are ranked by relevance with a 3x boost for symbol matches — so `ns -- "EventStore"` ranks the file where `EventStore` is defined above files that merely import it.

The index is file-level, not line-level. BM25 tells the agent "look in this file." Context lines are extracted post-search by re-reading the top result files, which is fast since only a handful of files are scanned.

### Language support

ns indexes **all text files** in your repository — any language, any file type. Every file gets full-text BM25 search. You can search a Ruby, C++, or Haskell codebase without any special configuration.

For a subset of languages, ns also parses the source with [tree-sitter](https://tree-sitter.github.io/) to extract **symbol names** (functions, classes, types, etc.). These symbols are indexed in a separate field with a **3x relevance boost**, so searching `"EventStore"` ranks the file where `EventStore` is *defined* above files that merely mention it in a comment or import.

**Languages with symbol extraction:**

| Language | Extensions | Symbols extracted |
|----------|-----------|-------------------|
| Rust | `.rs` | functions, structs, enums, traits, impl types, consts, type aliases |
| TypeScript | `.ts` `.tsx` `.mts` `.cts` | functions, classes, interfaces, types, enums, methods, top-level consts |
| JavaScript | `.js` `.jsx` `.mjs` `.cjs` | functions, classes, methods, top-level consts |
| Python | `.py` `.pyi` | functions, classes (including decorated) |
| Go | `.go` | functions, methods, types, consts |
| Elixir | `.ex` `.exs` | modules, functions (def/defp), macros, protocols, impls, guards, delegates, structs |

**What this means in practice:**

- **Supported language:** `ns --sym -- "EventStore"` finds where `EventStore` is defined. `ns -- "EventStore"` returns the definition file first, then files that reference it.
- **Any other language:** `ns -- "EventStore"` still works — it searches file content via BM25. Results are ranked by term frequency and document length, but without the symbol definition boost. `--sym` will return no results since there are no extracted symbols.

Both modes use the same index. No configuration needed — just `ns index` and search.

## Commands

### Search (default)

```
ns [OPTIONS] -- "<QUERY>"
```

Search is the default command. No subcommand needed. Use `--` before the query to separate flags from the search term — this avoids ambiguity when a query matches a subcommand name (e.g., `"index"`, `"status"`), and mirrors how `rg` separates options from patterns.

```bash
ns -- "EventStore"                  # basic search
ns -t rust -- "handler"             # filter by language
ns -g "src/api/*" -- "config"       # filter by path glob
ns --sym -- "Event"                 # search symbol names only
ns --fuzzy -- "EvntStore"           # typo-tolerant search (Levenshtein distance 1)
ns -l -- "middleware"               # file paths only
ns --json -- "UserRepo"             # JSON output (for programmatic use)
ns -m 20 -- "store"                 # return up to 20 results
ns -C 3 -- "handler"               # 3 lines of context around matches
```

For simple queries that don't collide with subcommand names, `ns "query"` still works. There is also an explicit `ns search "query"` subcommand as an alternative.

**Flags:**

| Flag | Description |
|------|-------------|
| `-t, --type <LANG>` | Filter by language (`rust`, `python`, `typescript`, etc.) |
| `-g, --glob <PATTERN>` | Filter to files matching glob pattern |
| `-l, --files` | Print file paths only, no context lines |
| `-m, --max-count <N>` | Max results to return (default: 10) |
| `-C, --context <N>` | Lines of context around matches (default: 1) |
| `--sym` | Search symbol names only (functions, types, traits, etc.) |
| `--fuzzy` | Enable typo tolerance |
| `--json` | Output as JSON |
| `-i, --ignore-case` | Accepted for rg compatibility (search is always case-insensitive) |

**Exit codes:** `0` = results found, `1` = no results or error.

### Index

```
ns index [OPTIONS]
```

Build or rebuild the search index.

```bash
ns index                          # full index
ns index --incremental            # only re-index changed files
ns index --root /path/to/repo     # specify repo root
ns index --max-file-size 2097152  # skip files > 2MB
```

**Incremental indexing** uses `git diff` (in git repos) or file mtime (elsewhere) to detect changes. Only added, modified, and deleted files are processed.

### Status

```
ns status
```

Shows index metadata: file count, last indexed time, schema version, index size, git commit.

### Hooks

```
ns hooks install
ns hooks remove
```

`ns hooks install` adds git hooks (`post-commit`, `post-merge`, `post-checkout`) that run `ns index --incremental` in the background after every commit, merge, or branch switch. This keeps the index fresh without manual intervention.

`ns hooks remove` removes them. If a hook had pre-existing content before ns was installed, only the ns lines are removed — your original hook is preserved.

## Output formats

**Text (default):**

```
 [1] src/event_store.rs        (score: 12.4, lang: rust)
     42: pub struct EventStore {
     43:     db: DatabasePool,
```

**JSON (`--json`):**

```json
{
  "results": [
    {
      "path": "src/event_store.rs",
      "score": 12.4,
      "lang": "rust",
      "matched_symbols": ["EventStore"],
      "lines": [
        { "num": 42, "text": "pub struct EventStore {" }
      ]
    }
  ],
  "stats": { "total_results": 1, "files_searched": 847, "elapsed_ms": 2 }
}
```

**Files only (`-l`):**

```
src/event_store.rs
src/reconciliation.rs
```

## Ripgrep compatibility

ns mirrors ripgrep's flags where semantics overlap. If you know rg, you know ns:

| rg | ns | Notes |
|----|----|-------|
| `rg -- "pattern"` | `ns -- "pattern"` | Positional query |
| `rg -t rust -- "pattern"` | `ns -t rust -- "pattern"` | Language filter |
| `rg -g "src/**" -- "pattern"` | `ns -g "src/**" -- "pattern"` | Glob filter |
| `rg -l -- "pattern"` | `ns -l -- "pattern"` | Files only |
| `rg -m 5 -- "pattern"` | `ns -m 5 -- "pattern"` | Max results |
| `rg -C 3 -- "pattern"` | `ns -C 3 -- "pattern"` | Context lines |
| `rg --json -- "pattern"` | `ns --json -- "pattern"` | JSON output |
| — | `ns --sym -- "pattern"` | Symbol-only search (ns-unique) |
| — | `ns --fuzzy -- "pattern"` | Typo tolerance (ns-unique) |

ns is not a regex engine. If you need regex or line-level pattern matching, use rg. ns is for ranked, relevance-ordered search.

## Usage with LLM agents

ns is designed to be called by LLM coding agents (Claude Code, Cursor, Aider, custom agents) as a drop-in replacement for grep/ripgrep when ranked results matter.

### Setup

```bash
# Build and install
cargo build --release
cp target/release/ns /usr/local/bin/

# In your project, build the index once
cd /path/to/project
ns index

# Optional: auto-maintain the index on every commit
ns hooks install
```

### Agent instructions

Add something like this to your agent's system prompt, `CLAUDE.md`, or equivalent instructions file:

```
## Code search

Use `ns` for searching the codebase. It returns results ranked by relevance
with symbol definitions (functions, classes, types) boosted above plain text matches.

Always use `--` before the query to separate flags from the search term:
- `ns -- "query"` — search by relevance. Best default choice.
- `ns --sym -- "query"` — find where a symbol is defined (function, class, type).
- `ns -t rust -- "query"` — limit search to a specific language.
- `ns -g "src/api/*" -- "query"` — limit search to a path pattern.
- `ns --json -- "query"` — structured output with scores and matched symbols.
- `ns --fuzzy -- "query"` — if exact search returns nothing, retry with typo tolerance.
- `ns -l -- "query"` — get just file paths (useful for batch operations).
- `ns index --incremental` — re-index if results seem stale.

ns flags mirror ripgrep. If you know rg flags, they work the same way in ns.
```

### Why not just use ripgrep?

For targeted lookups where you know the exact string, rg is fine. ns is better when:

- **You're exploring.** `ns -- "authentication"` returns the 10 most relevant files ranked by BM25 score. `rg "authentication"` returns every file containing the string, unranked — could be hundreds.
- **You're looking for definitions.** `ns --sym -- "UserRepository"` finds where `UserRepository` is defined, not every file that imports it.
- **You're dealing with naming variations.** `ns --fuzzy -- "EventStore"` catches `EvnetStore` typos and near-matches.
- **You want structured output.** `ns --json -- "handler"` returns scores, matched symbols, and context lines in a format agents can parse without regex.

Use both. rg for precise pattern matching, ns for "find me the relevant files."

### Example: Claude Code with CLAUDE.md

```markdown
# CLAUDE.md

## Tools

This project has `ns` (Nano Search) installed for ranked code search.
Prefer `ns` over `grep`/`rg` when looking for relevant files or symbol definitions.

- Find relevant files: `ns -- "query"`
- Find definitions: `ns --sym -- "ClassName"`
- Structured results: `ns --json -- "query"`
- Rebuild index after large changes: `ns index --incremental`
```

## The `.ns/` directory

The index lives in `.ns/` at the repo root. Add it to `.gitignore`:

```
echo '.ns/' >> .gitignore
```

ns will warn you if `.ns/` isn't gitignored.

## Dependencies

ns compiles to a single binary with no runtime dependencies. Build dependencies:

- [tantivy](https://github.com/quickwit-oss/tantivy) — inverted index, BM25 scoring, storage
- [tree-sitter](https://tree-sitter.github.io/) — AST parsing for symbol extraction
- [ignore](https://crates.io/crates/ignore) — .gitignore-aware file walking (same crate ripgrep uses)
- [clap](https://crates.io/crates/clap) — CLI argument parsing

## License

MIT
