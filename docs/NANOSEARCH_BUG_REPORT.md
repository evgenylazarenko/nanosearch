# Nanosearch (`ns`) Bug Report

Date: 2026-02-12  
Reporter: Codex test run

## Environment

- Host OS: `Darwin 22.4.0 (arm64)`
- Repo used for testing: `/Users/evgeny/ClaudeSandbox/Skylane`
- Tested binary: `ns 0.1.0` (`/usr/local/bin/ns`)
- Repo HEAD during test: `e597473`

## Bug 1: Panic on Broken Pipe During JSON Output

### Severity
High (process panic in normal shell piping workflows)

### Reproduction steps

```bash
cd /Users/evgeny/ClaudeSandbox/Skylane
ns "Provider" --json | false
echo $?
```

### Actual behavior

- `ns` panics with Rust runtime output:

```text
thread 'main' (...) panicked at library/std/src/io/stdio.rs:1165:9:
failed printing to stdout: Broken pipe (os error 32)
```

- Exit code observed: `1`

### Expected behavior

- No panic output.
- Graceful handling of `EPIPE` when stdout closes early (common in pipelines like `| head`, `| jq`, etc.).

## Bug 2: Incremental Indexing Re-adds Files and Causes Duplicate Search Hits

### Severity
High (index consistency/ranking correctness risk)

### Reproduction steps

```bash
cd /Users/evgeny/ClaudeSandbox/Skylane

ns index
ns status

ns index --incremental
ns status

ns index --incremental
ns status
```

### Actual behavior

- Full index reports `Indexed 244 files`.
- First incremental run reports `Incremental update: 7 added, 0 modified, 0 deleted`.
- Second incremental run again reports `Incremental update: 7 added, 0 modified, 0 deleted`.
- `ns status` file counts increase each run:
  - After full index: `files indexed  : 244`
  - After incremental #1: `files indexed  : 251`
  - After incremental #2: `files indexed  : 258`

This suggests repeated insertion of already-indexed documents instead of idempotent update behavior.

### Correlation observed during triage

- The repeated `+7` on incremental runs matched the count of untracked repo files/paths at test time (excluding `.ns/` internals).
- This suggests incremental indexing may be repeatedly treating untracked files as newly added across runs in this workspace.
- This is currently an observed correlation, not a confirmed root cause.

### Additional evidence (duplicate paths in search results)

```bash
cd /Users/evgeny/ClaudeSandbox/Skylane
ns -l -m 300 -- "the" >/tmp/ns_the_paths.txt
sort /tmp/ns_the_paths.txt | uniq -cd | sort -nr | head -n 10
```

Observed duplicates include:

```text
3 docs/implementation_docs/CLI-Worktree-Plan.md
3 docs/checkpoint_updates/checkpoint_normalization_findings_2026-02-06.md
3 autobuild-old
3 autobuild
```

### Expected behavior

- Re-running `ns index --incremental` without content changes should be a no-op (`Index is up to date.` or `0 added, 0 modified, 0 deleted`).
- Search results should not contain duplicate identical file paths unless duplicates are explicitly part of output design (not documented).

## Bug 3: Query String Matching Subcommand Name Is Parsed as Subcommand

### Severity
Medium (usability/compatibility friction)

### Reproduction steps

```bash
cd /Users/evgeny/ClaudeSandbox/Skylane
ns "index" -l
echo $?
```

### Actual behavior

- `index` is interpreted as subcommand, not query:

```text
error: unexpected argument '-l' found
Usage: ns index [OPTIONS]
```

- Exit code observed: `2`.

Workaround:

```bash
ns -l -- "index"
```

### Expected behavior

Either:

1. Quoted positional `"index"` is treated as search query in default search mode, or
2. Behavior is clearly documented with explicit requirement to use `--` when query matches a subcommand token.

## Notes

- README was used as the behavioral spec source: `/Users/evgeny/ClaudeSandbox/nanosearch/README.md`.
- `nanosearch` alias/command was not present in PATH during this test; installed executable was `ns`.
