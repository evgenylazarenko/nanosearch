# Bug Fixes — Implementation Plan

Fixes for bugs reported in `NANOSEARCH_BUG_REPORT.md`.

---

## Bug 1: Panic on Broken Pipe During JSON Output

### Severity: High

### Root Cause

`cmd/search.rs` uses `print!("{}", output)` to write search results to stdout. When stdout is a pipe and the consumer closes early (e.g., `ns "Provider" --json | head`), the next write to stdout triggers `EPIPE`. Rust's `print!`/`println!` macros panic on write failure by design.

The same issue exists in `main.rs` line 28 (`println!()` after printing help).

### Fix

Install a broken-pipe handler at the top of `main()`. The idiomatic Rust approach:

**Option A (recommended): Reset SIGPIPE to default behavior.**

Unix processes inherit `SIG_IGN` for SIGPIPE from some parent processes. Resetting to `SIG_DFL` causes the process to silently terminate on broken pipe — the standard behavior for CLI tools (this is what `grep`, `cat`, `head` all do).

```rust
// main.rs, top of main()
#[cfg(unix)]
{
    // Reset SIGPIPE to default (terminate silently) so piping works.
    // Rust runtime sets SIG_IGN by default, which causes panics on write.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}
```

This requires adding `libc` as a dependency, but it's already a transitive dep of tantivy/tree-sitter so adds no new code.

**Option B: Catch write errors.**

Replace `print!` with `write!` to `io::stdout()` and handle `ErrorKind::BrokenPipe` by exiting silently. More verbose, must be applied at every print site.

**Decision: Option A.** It's the standard solution used by ripgrep, bat, fd, and most Rust CLI tools. One line, covers all print sites.

### Files Modified

| File | Change |
|------|--------|
| `Cargo.toml` | Add `libc = "0.2"` |
| `src/main.rs` | Add SIGPIPE reset at top of `main()` |

### Tests

- Integration test: pipe `ns` output to `false` (or a process that closes stdin immediately), assert exit code is 0 or 141 (SIGPIPE), and assert no panic text on stderr.

---

## Bug 2: Incremental Indexing Re-adds Untracked Files

### Severity: High

### Root Cause

The bug is in `detect_changes_git_uncommitted()` (line 200-216 of `incremental.rs`). When the indexed commit matches HEAD (same-commit path), this function:

1. Runs `git diff --name-status HEAD` to find uncommitted changes — correct
2. Runs `git ls-files --others --exclude-standard` to find untracked files — **this is the problem**

Untracked files are always reported by `git ls-files --others` regardless of whether they're already in the index. On the first incremental run after a full index, any untracked files get added to the `added` list because they're returned by `git ls-files --others`. But they're already in the index (the full indexer walked them via the `ignore` crate). So they get re-added without being deleted first → duplicates.

On the second incremental run, the same thing happens. The untracked files are still untracked (user hasn't committed them), so `git ls-files --others` returns them again → more duplicates.

The core issue: **for the same-commit path, the code has no way to know which untracked files are already indexed.** It assumes all untracked files are new, but the full indexer already indexed them.

### Fix

Before adding an untracked file to the `added` list, check whether it's already in the index. This requires querying the existing index for the path.

**Approach:** Pass the set of already-indexed paths into `detect_changes_git_uncommitted()`. Build this set once from the tantivy index (same technique used in `detect_changes_mtime()`), then skip untracked files that are already present.

Concretely:

1. In `detect_changes()`, build an `indexed_paths: HashSet<String>` from the tantivy index before dispatching to git-based or mtime-based detection.
2. Pass `&indexed_paths` to `detect_changes_git_uncommitted()` and `detect_changes_git()`.
3. In `detect_changes_git_uncommitted()`, when iterating untracked files, skip any path that's in `indexed_paths`.
4. In `detect_changes_git()`, the same — the merged working changes should also exclude already-indexed untracked files.

```rust
// In detect_changes_git_uncommitted:
if untracked_output.status.success() {
    let untracked = String::from_utf8_lossy(&untracked_output.stdout);
    for line in untracked.lines() {
        let path = line.trim();
        if !path.is_empty()
            && !changes.added.contains(&path.to_string())
            && !indexed_paths.contains(path)  // ← NEW: skip if already indexed
        {
            changes.added.push(path.to_string());
        }
    }
}
```

But this alone isn't sufficient — we also need to detect **modifications** to untracked files that are already indexed. An untracked file could have changed since the last index. So the logic should be:

- Untracked file NOT in index → `added`
- Untracked file in index AND mtime > indexed_at → `modified`
- Untracked file in index AND mtime <= indexed_at → skip (already up to date)

This gives correct behavior for all three incremental runs:
- Run 1 after full index: untracked files are in index, mtimes match → skip → 0 added
- Run after editing an untracked file: mtime is newer → modified → delete + re-add
- Run after creating a new untracked file: not in index → added

### Alternative considered

Use `git ls-files --others` only on the first incremental run (when there's no prior incremental commit). Rejected — the problem would reappear whenever HEAD doesn't change between runs (common when editing without committing).

### Helper: get_indexed_paths()

Extract the path-reading logic from `detect_changes_mtime()` into a shared helper since both code paths need it now:

```rust
fn get_indexed_paths(index: &tantivy::Index) -> Result<HashSet<String>, NsError> {
    let reader = index
        .reader_builder()
        .reload_policy(ReloadPolicy::Manual)
        .try_into()?;
    let searcher = reader.searcher();
    let schema = index.schema();
    let path_f = path_field(&schema);

    let mut paths = HashSet::new();
    for segment_reader in searcher.segment_readers() {
        let store_reader = segment_reader.get_store_reader(1)?;
        for doc_id in 0..segment_reader.num_docs() {
            if let Ok(doc) = store_reader.get::<TantivyDocument>(doc_id) {
                if let Some(val) = doc.get_first(path_f) {
                    if let Some(path_str) = val.as_str() {
                        paths.insert(path_str.to_string());
                    }
                }
            }
        }
    }
    Ok(paths)
}
```

### Files Modified

| File | Change |
|------|--------|
| `src/indexer/incremental.rs` | Extract `get_indexed_paths()`, pass indexed paths into git change detection, add mtime check for already-indexed untracked files |

### Tests

1. **Unit test:** `detect_changes_git_uncommitted` with mocked indexed_paths containing an untracked file → should not appear in `added`.
2. **Integration test:** Full index → incremental → incremental. Assert file count stays constant across all three runs. Assert second incremental reports `0 added, 0 modified, 0 deleted`.
3. **Integration test:** Full index → create new untracked file → incremental. Assert `1 added`. Run incremental again → `0 added`.

---

## Bug 3: Query Matching Subcommand Name Parsed as Subcommand

### Severity: Medium

### Root Cause

Clap's `#[command(subcommand)]` parsing is greedy. When the user types `ns "index" -l`, clap sees `index` as a positional argument and matches it against the `Index` subcommand before considering it as the `query` field. Quoting doesn't help because by the time clap sees the argument, shell quoting has been stripped.

This is a known clap design tension when mixing subcommands with positional arguments.

### Fix

**Option A: Add `ns search` as an explicit subcommand.**

Add `Search` as a subcommand that takes the query and all search flags. The default (no subcommand) behavior remains unchanged for unambiguous queries, but users/agents can use `ns search "index"` to force search mode.

```rust
#[derive(Subcommand)]
pub enum Command {
    /// Search the index (use when query matches a subcommand name)
    Search(SearchSubArgs),
    /// Build or update the search index
    Index(IndexArgs),
    /// Show index status
    Status,
    /// Manage git hooks
    Hooks { ... },
}
```

This is the cleanest solution:
- `ns "handler"` → works as before (default search)
- `ns search "index"` → unambiguous search for "index"
- `ns index` → index subcommand
- Fully backward compatible

**Option B: Use clap's `#[command(subcommand_negates_reqs)]` or external subcommand.**

More complex, fragile across clap versions.

**Option C: Document `--` workaround only.**

The workaround `ns -- "index" -l` already works. But agents won't know to use it unless explicitly told, and it's a poor UX.

**Decision: Option A.** Add `ns search` as an explicit subcommand. Document it as the escape hatch for queries that collide with subcommand names. Update README.

### Files Modified

| File | Change |
|------|--------|
| `src/cmd/mod.rs` | Add `Search(SearchSubArgs)` variant to `Command` enum, define `SearchSubArgs` |
| `src/main.rs` | Handle `Command::Search` dispatch |
| `README.md` | Document `ns search` as escape hatch, add note about `--` |

### Tests

- Integration test: `ns search "index" -l` exits 0 (or 1 for no results) without subcommand error.
- Integration test: `ns -- "index" -l` also works.
- Integration test: `ns "handler" -l` still works (backward compat).

---

## Implementation Order

1. **Bug 1 (broken pipe)** — Smallest change, highest impact, no risk of regression. One line + one dep.
2. **Bug 3 (subcommand collision)** — CLI-level fix, no index logic changes. Low regression risk.
3. **Bug 2 (incremental duplicates)** — Largest change, touches index logic. Needs careful testing.

## Estimated Test Count

Current: 154 tests. Expected new: ~6-8 (2 for bug 1, 2-3 for bug 2, 2-3 for bug 3). Total: ~160-162.
