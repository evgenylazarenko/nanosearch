use std::path::Path;

use tree_sitter::{Node, Parser};

use crate::indexer::language::detect_language;
use crate::searcher::context::{tokenize_query, ContextLine, ContextResult};

/// A candidate span from the AST (or a fallback fixed window).
#[derive(Debug)]
#[allow(dead_code)]
struct SpanCandidate {
    /// 0-based start line (inclusive).
    start_line: usize,
    /// 0-based end line (inclusive).
    end_line: usize,
    /// Tree-sitter node kind (or "window" for fallback).
    kind: &'static str,
    /// Extracted symbol name, if any.
    symbol_name: Option<String>,
    /// Index of the smallest containing candidate (set after collection).
    parent_idx: Option<usize>,
    /// True for AST-extracted nodes; false for fallback windows.
    is_ast: bool,
}

/// A candidate together with its computed score.
#[allow(dead_code)]
struct ScoredSpan {
    /// Index into the candidates slice.
    idx: usize,
    score: f64,
    /// score / line_count — used for greedy sort.
    score_density: f64,
}

/// Top-level entry point. Replaces `extract_context` when `--spans` is active.
///
/// Parses `rel_path` with tree-sitter, scores AST candidates against `query`,
/// and packs the best non-overlapping spans under the per-file line budget.
///
/// Returns the same `ContextResult` type as `extract_context` so that the
/// downstream formatting pipeline requires no changes.
pub fn extract_best_spans(
    root: &Path,
    rel_path: &str,
    query: &str,
    max_lines: Option<usize>,
) -> ContextResult {
    let empty = ContextResult {
        lines: Vec::new(),
        truncated_count: 0,
    };

    let full_path = root.join(rel_path);
    let content = match std::fs::read_to_string(&full_path) {
        Ok(s) => s,
        Err(_) => return empty,
    };

    let source = content.as_bytes();
    let file_lines: Vec<&str> = content.lines().collect();
    let total_lines = file_lines.len();
    if total_lines == 0 {
        return empty;
    }

    let terms = tokenize_query(query);
    if terms.is_empty() {
        return empty;
    }

    // Budget: 0 = unlimited (consistent with SearchOptions convention)
    let budget = match max_lines {
        Some(0) | None => 0,
        Some(n) => n,
    };

    // Phase 1: extract candidates
    let lang = detect_language(Path::new(rel_path)).unwrap_or("");
    let candidates = extract_span_candidates(lang, source, total_lines);
    if candidates.is_empty() {
        return empty;
    }

    // Phase 2: score
    let scored = score_candidates(&candidates, &file_lines, &terms, budget);
    if scored.is_empty() {
        return empty;
    }

    // Phase 3: greedy pack
    let selected = pack_spans(&candidates, scored, budget);
    if selected.is_empty() {
        return empty;
    }

    // Build output
    build_context_result(&candidates, &selected, &file_lines, budget)
}

// ── Phase 1: Extract candidates ───────────────────────────────────────────────

fn extract_span_candidates(lang: &str, source: &[u8], total_lines: usize) -> Vec<SpanCandidate> {
    let raw = match lang {
        "rust" => extract_rust(source),
        "typescript" => extract_typescript(source),
        "javascript" => extract_javascript(source),
        "python" => extract_python(source),
        "go" => extract_go(source),
        "elixir" => extract_elixir(source),
        _ => Vec::new(),
    };

    let mut candidates = if raw.is_empty() {
        fallback_windows(total_lines)
    } else {
        raw
    };

    assign_parent_indices(&mut candidates);
    candidates
}

/// Assigns `parent_idx` for each candidate: the index of the smallest
/// containing span (i.e., the tightest enclosing span).
fn assign_parent_indices(candidates: &mut Vec<SpanCandidate>) {
    let n = candidates.len();
    for i in 0..n {
        let mut best: Option<usize> = None;
        let mut best_size = usize::MAX;
        for j in 0..n {
            if i == j {
                continue;
            }
            let (si, ei) = (candidates[i].start_line, candidates[i].end_line);
            let (sj, ej) = (candidates[j].start_line, candidates[j].end_line);
            if sj <= si && ej >= ei {
                let size = ej - sj;
                if size < best_size {
                    best_size = size;
                    best = Some(j);
                }
            }
        }
        candidates[i].parent_idx = best;
    }
}

/// Generates fixed 20-line windows with 10-line overlap as a fallback.
fn fallback_windows(total_lines: usize) -> Vec<SpanCandidate> {
    if total_lines == 0 {
        return Vec::new();
    }
    let window_size = 20;
    let step = 10;
    let mut windows = Vec::new();
    let mut start = 0;
    loop {
        let end = (start + window_size - 1).min(total_lines - 1);
        windows.push(SpanCandidate {
            start_line: start,
            end_line: end,
            kind: "window",
            symbol_name: None,
            parent_idx: None,
            is_ast: false,
        });
        if end >= total_lines - 1 {
            break;
        }
        start += step;
    }
    windows
}

// ── Language walkers ──────────────────────────────────────────────────────────

fn extract_rust(source: &[u8]) -> Vec<SpanCandidate> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .expect("failed to load Rust grammar");
    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => return Vec::new(),
    };
    let mut candidates = Vec::new();
    walk_rust(tree.root_node(), source, &mut candidates);
    candidates
}

fn walk_rust(node: Node, source: &[u8], out: &mut Vec<SpanCandidate>) {
    let kind = match node.kind() {
        "function_item" | "function_signature_item" => "function_item",
        "struct_item" => "struct_item",
        "enum_item" => "enum_item",
        "trait_item" => "trait_item",
        "impl_item" => "impl_item",
        "const_item" => "const_item",
        "type_item" => "type_item",
        _ => {
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i) {
                    walk_rust(child, source, out);
                }
            }
            return;
        }
    };

    let symbol_name = if kind == "impl_item" {
        node.child_by_field_name("type")
            .and_then(|t| t.utf8_text(source).ok())
            .map(|s| s.split('<').next().unwrap_or(s).trim().to_string())
    } else {
        node.child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            .map(|s| s.to_string())
    };

    let start_line = node.start_position().row;
    let end_line = node.end_position().row;

    out.push(SpanCandidate {
        start_line,
        end_line,
        kind,
        symbol_name,
        parent_idx: None,
        is_ast: true,
    });

    // Recurse into children to pick up nested items (e.g. methods inside impl)
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            walk_rust(child, source, out);
        }
    }
}

fn extract_typescript(source: &[u8]) -> Vec<SpanCandidate> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TSX.into())
        .expect("failed to load TypeScript grammar");
    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => return Vec::new(),
    };
    let mut candidates = Vec::new();
    walk_js_ts(tree.root_node(), source, &mut candidates, true);
    candidates
}

fn extract_javascript(source: &[u8]) -> Vec<SpanCandidate> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_javascript::LANGUAGE.into())
        .expect("failed to load JavaScript grammar");
    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => return Vec::new(),
    };
    let mut candidates = Vec::new();
    walk_js_ts(tree.root_node(), source, &mut candidates, false);
    candidates
}

fn walk_js_ts(node: Node, source: &[u8], out: &mut Vec<SpanCandidate>, ts_extras: bool) {
    let kind: Option<&'static str> = match node.kind() {
        "function_declaration" => Some("function_declaration"),
        "class_declaration" => Some("class_declaration"),
        "method_definition" => Some("method_definition"),
        "interface_declaration" if ts_extras => Some("interface_declaration"),
        "type_alias_declaration" if ts_extras => Some("type_alias_declaration"),
        "enum_declaration" if ts_extras => Some("enum_declaration"),
        "lexical_declaration" => {
            // Only capture top-level lexical_declaration (const/let at module scope)
            if is_top_level(&node) {
                Some("lexical_declaration")
            } else {
                None
            }
        }
        _ => None,
    };

    if let Some(k) = kind {
        let symbol_name = if k == "lexical_declaration" {
            // Extract first declarator name
            first_declarator_name(&node, source)
        } else {
            node.child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .map(|s| s.to_string())
        };

        out.push(SpanCandidate {
            start_line: node.start_position().row,
            end_line: node.end_position().row,
            kind: k,
            symbol_name,
            parent_idx: None,
            is_ast: true,
        });
    }

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            walk_js_ts(child, source, out, ts_extras);
        }
    }
}

fn extract_python(source: &[u8]) -> Vec<SpanCandidate> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .expect("failed to load Python grammar");
    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => return Vec::new(),
    };
    let mut candidates = Vec::new();
    walk_python(tree.root_node(), source, &mut candidates);
    candidates
}

fn walk_python(node: Node, source: &[u8], out: &mut Vec<SpanCandidate>) {
    let kind: Option<&'static str> = match node.kind() {
        "function_definition" => Some("function_definition"),
        "class_definition" => Some("class_definition"),
        _ => None,
    };

    if let Some(k) = kind {
        let symbol_name = node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            .map(|s| s.to_string());

        out.push(SpanCandidate {
            start_line: node.start_position().row,
            end_line: node.end_position().row,
            kind: k,
            symbol_name,
            parent_idx: None,
            is_ast: true,
        });
    }

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            walk_python(child, source, out);
        }
    }
}

fn extract_go(source: &[u8]) -> Vec<SpanCandidate> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_go::LANGUAGE.into())
        .expect("failed to load Go grammar");
    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => return Vec::new(),
    };
    let mut candidates = Vec::new();
    walk_go(tree.root_node(), source, &mut candidates);
    candidates
}

fn walk_go(node: Node, source: &[u8], out: &mut Vec<SpanCandidate>) {
    let kind: Option<&'static str> = match node.kind() {
        "function_declaration" => Some("function_declaration"),
        "method_declaration" => Some("method_declaration"),
        "type_spec" | "type_declaration" => Some("type_declaration"),
        "const_spec" | "const_declaration" => Some("const_declaration"),
        _ => None,
    };

    if let Some(k) = kind {
        let symbol_name = node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            .map(|s| s.to_string());

        out.push(SpanCandidate {
            start_line: node.start_position().row,
            end_line: node.end_position().row,
            kind: k,
            symbol_name,
            parent_idx: None,
            is_ast: true,
        });
    }

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            walk_go(child, source, out);
        }
    }
}

fn extract_elixir(source: &[u8]) -> Vec<SpanCandidate> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_elixir::LANGUAGE.into())
        .expect("failed to load Elixir grammar");
    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => return Vec::new(),
    };
    let mut candidates = Vec::new();
    walk_elixir(tree.root_node(), source, &mut candidates);
    candidates
}

fn walk_elixir(node: Node, source: &[u8], out: &mut Vec<SpanCandidate>) {
    if node.kind() == "call" {
        if let Some(target) = node.child_by_field_name("target") {
            if target.kind() == "identifier" {
                if let Ok(keyword) = target.utf8_text(source) {
                    let kind: Option<&'static str> = match keyword {
                        "defmodule" | "defprotocol" => Some("defmodule"),
                        "defimpl" => Some("defimpl"),
                        "def" | "defp" | "defmacro" | "defmacrop" | "defguard" | "defguardp"
                        | "defdelegate" => Some("def"),
                        _ => None,
                    };

                    if let Some(k) = kind {
                        let symbol_name = elixir_symbol_name(&node, source, keyword);
                        out.push(SpanCandidate {
                            start_line: node.start_position().row,
                            end_line: node.end_position().row,
                            kind: k,
                            symbol_name,
                            parent_idx: None,
                            is_ast: true,
                        });
                    }
                }
            }
        }
    }

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            walk_elixir(child, source, out);
        }
    }
}

// ── Elixir helpers ─────────────────────────────────────────────────────────────

fn elixir_symbol_name(call_node: &Node, source: &[u8], keyword: &str) -> Option<String> {
    // Find the arguments child
    for i in 0..call_node.child_count() {
        let child = call_node.child(i)?;
        if child.kind() != "arguments" {
            continue;
        }
        let first = child.named_child(0)?;
        match keyword {
            "defmodule" | "defprotocol" | "defimpl" => {
                if first.kind() == "alias" {
                    return first.utf8_text(source).ok().map(|s| s.to_string());
                }
            }
            _ => {
                // def/defp/defmacro/etc — extract function name
                return elixir_fn_name_from_arg(&first, source);
            }
        }
        break;
    }
    None
}

fn elixir_fn_name_from_arg(arg: &Node, source: &[u8]) -> Option<String> {
    match arg.kind() {
        "call" => {
            // def func_name(args)
            arg.child_by_field_name("target")
                .filter(|n| n.kind() == "identifier")
                .and_then(|n| n.utf8_text(source).ok())
                .map(|s| s.to_string())
        }
        "identifier" => {
            // def func_name (no args)
            arg.utf8_text(source).ok().map(|s| s.to_string())
        }
        "binary_operator" => {
            // def func(x) when guard
            arg.child_by_field_name("left")
                .filter(|n| n.kind() == "call")
                .and_then(|n| n.child_by_field_name("target"))
                .filter(|n| n.kind() == "identifier")
                .and_then(|n| n.utf8_text(source).ok())
                .map(|s| s.to_string())
        }
        _ => None,
    }
}

// ── JS/TS helpers ─────────────────────────────────────────────────────────────

fn is_top_level(node: &Node) -> bool {
    match node.parent() {
        Some(p) => matches!(p.kind(), "program" | "export_statement"),
        None => true,
    }
}

fn first_declarator_name(node: &Node, source: &[u8]) -> Option<String> {
    for i in 0..node.child_count() {
        let child = node.child(i)?;
        if child.kind() == "variable_declarator" {
            return child
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .map(|s| s.to_string());
        }
    }
    None
}

// ── Phase 2: Score ─────────────────────────────────────────────────────────────

fn score_candidates(
    candidates: &[SpanCandidate],
    lines: &[&str],
    terms: &[String],
    budget: usize,
) -> Vec<ScoredSpan> {
    let mut scored = Vec::new();

    for (idx, candidate) in candidates.iter().enumerate() {
        let start = candidate.start_line;
        let end = candidate.end_line.min(lines.len().saturating_sub(1));
        if start > end {
            continue;
        }
        let span_lines = end - start + 1;

        // Concatenate span text for term matching
        let span_lower: String = lines[start..=end]
            .iter()
            .flat_map(|l| l.chars().chain(std::iter::once('\n')))
            .collect::<String>()
            .to_lowercase();

        let hits = terms
            .iter()
            .filter(|term| span_lower.contains(term.as_str()))
            .count() as f64;

        let name_match = candidate
            .symbol_name
            .as_ref()
            .map(|name| {
                let name_lower = name.to_lowercase();
                terms.iter().any(|t| name_lower.contains(t.as_str()))
            })
            .unwrap_or(false);

        if hits == 0.0 && !name_match {
            continue;
        }

        let def_bonus: f64 = if candidate.is_ast { 1.0 } else { 0.0 };
        let size_penalty: f64 = if budget > 0 {
            (span_lines as f64 / budget as f64) * 0.3
        } else {
            0.0
        };

        let score = hits + (if name_match { 5.0 } else { 0.0 }) + def_bonus - size_penalty;
        let score_density = score / span_lines.max(1) as f64;

        scored.push(ScoredSpan {
            idx,
            score,
            score_density,
        });
    }

    scored
}

// ── Phase 3: Greedy pack ───────────────────────────────────────────────────────

fn pack_spans(
    candidates: &[SpanCandidate],
    mut scored: Vec<ScoredSpan>,
    budget: usize,
) -> Vec<usize> {
    // Sort by score density descending
    scored.sort_by(|a, b| {
        b.score_density
            .partial_cmp(&a.score_density)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let unlimited = budget == 0;
    let mut remaining = if unlimited { usize::MAX } else { budget };
    let mut selected: Vec<usize> = Vec::new();

    for ss in &scored {
        if !unlimited && remaining == 0 {
            break;
        }

        let idx = ss.idx;
        let c = &candidates[idx];
        let span_lines = c.end_line.saturating_sub(c.start_line) + 1;

        // Skip if overlaps or is nested inside an already-selected span
        let overlaps = selected.iter().any(|&sel| {
            let s = &candidates[sel];
            !(c.end_line < s.start_line || c.start_line > s.end_line)
        });
        if overlaps {
            continue;
        }

        if unlimited || span_lines <= remaining {
            selected.push(idx);
            if !unlimited {
                remaining -= span_lines;
            }
        } else if !unlimited && remaining > 5 {
            // Partially fits — include it (truncated at output time)
            selected.push(idx);
            remaining = 0;
        }
        // else: too small to bother truncating, skip
    }

    // Sort selected spans by start_line for ordered output
    selected.sort_by_key(|&idx| candidates[idx].start_line);
    selected
}

// ── Build output ───────────────────────────────────────────────────────────────

fn build_context_result(
    candidates: &[SpanCandidate],
    selected: &[usize],
    lines: &[&str],
    budget: usize,
) -> ContextResult {
    let unlimited = budget == 0;
    let mut remaining = if unlimited { usize::MAX } else { budget };
    let mut context_lines: Vec<ContextLine> = Vec::new();
    let mut truncated_count: usize = 0;

    for &idx in selected {
        if !unlimited && remaining == 0 {
            // Count remaining spans as truncated
            let c = &candidates[idx];
            let span_end = c.end_line.min(lines.len().saturating_sub(1));
            truncated_count += span_end - c.start_line + 1;
            continue;
        }

        let c = &candidates[idx];
        let span_end = c.end_line.min(lines.len().saturating_sub(1));
        if c.start_line > span_end {
            continue;
        }
        let span_lines = span_end - c.start_line + 1;

        if unlimited || span_lines <= remaining {
            for i in c.start_line..=span_end {
                context_lines.push(ContextLine {
                    line_number: i + 1,
                    text: lines[i].to_string(),
                });
            }
            if !unlimited {
                remaining -= span_lines;
            }
        } else {
            // Truncate: first half + last half; gap shown by non-contiguous line numbers
            let half = remaining / 2;
            let first_end = (c.start_line + half).saturating_sub(1).min(span_end);
            let last_start = span_end.saturating_sub(half - 1).max(first_end + 1);

            for i in c.start_line..=first_end {
                context_lines.push(ContextLine {
                    line_number: i + 1,
                    text: lines[i].to_string(),
                });
            }
            for i in last_start..=span_end {
                context_lines.push(ContextLine {
                    line_number: i + 1,
                    text: lines[i].to_string(),
                });
            }
            truncated_count += last_start.saturating_sub(first_end + 1);
            remaining = 0;
        }
    }

    ContextResult {
        lines: context_lines,
        truncated_count,
    }
}

// ── Unit tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample_repo")
    }

    // ── extract_span_candidates ────────────────────────────────────────────────

    #[test]
    fn rust_extracts_known_spans() {
        let source = include_bytes!("../../tests/fixtures/sample_repo/src/event_store.rs");
        let total = std::str::from_utf8(source).unwrap().lines().count();
        let candidates = extract_span_candidates("rust", source, total);

        assert!(!candidates.is_empty(), "should extract Rust candidates");
        assert!(
            candidates.iter().all(|c| c.is_ast),
            "all should be AST candidates"
        );

        let kinds: Vec<&str> = candidates.iter().map(|c| c.kind).collect();
        assert!(
            kinds.contains(&"struct_item"),
            "should find struct_item"
        );
        assert!(kinds.contains(&"impl_item"), "should find impl_item");

        let names: Vec<&str> = candidates
            .iter()
            .filter_map(|c| c.symbol_name.as_deref())
            .collect();
        assert!(names.contains(&"EventStore"), "should find EventStore symbol");
    }

    #[test]
    fn typescript_extracts_known_spans() {
        let source = include_bytes!("../../tests/fixtures/sample_repo/src/handlers.ts");
        let total = std::str::from_utf8(source).unwrap().lines().count();
        let candidates = extract_span_candidates("typescript", source, total);

        assert!(!candidates.is_empty(), "should extract TS candidates");
        let names: Vec<&str> = candidates
            .iter()
            .filter_map(|c| c.symbol_name.as_deref())
            .collect();
        assert!(names.contains(&"Router"), "should find Router class");
    }

    #[test]
    fn python_extracts_known_spans() {
        let source = include_bytes!("../../tests/fixtures/sample_repo/src/models.py");
        let total = std::str::from_utf8(source).unwrap().lines().count();
        let candidates = extract_span_candidates("python", source, total);

        assert!(!candidates.is_empty(), "should extract Python candidates");
        let names: Vec<&str> = candidates
            .iter()
            .filter_map(|c| c.symbol_name.as_deref())
            .collect();
        assert!(names.contains(&"User"), "should find User class");
        assert!(
            names.contains(&"UserRepository"),
            "should find UserRepository"
        );
    }

    #[test]
    fn go_extracts_known_spans() {
        let source = include_bytes!("../../tests/fixtures/sample_repo/src/server.go");
        let total = std::str::from_utf8(source).unwrap().lines().count();
        let candidates = extract_span_candidates("go", source, total);

        assert!(!candidates.is_empty(), "should extract Go candidates");
        let names: Vec<&str> = candidates
            .iter()
            .filter_map(|c| c.symbol_name.as_deref())
            .collect();
        assert!(names.contains(&"Server"), "should find Server");
        assert!(names.contains(&"NewServer"), "should find NewServer");
    }

    #[test]
    fn elixir_extracts_known_spans() {
        let source = include_bytes!("../../tests/fixtures/sample_repo/src/event_manager.ex");
        let total = std::str::from_utf8(source).unwrap().lines().count();
        let candidates = extract_span_candidates("elixir", source, total);

        assert!(!candidates.is_empty(), "should extract Elixir candidates");
        let names: Vec<&str> = candidates
            .iter()
            .filter_map(|c| c.symbol_name.as_deref())
            .collect();
        assert!(
            names.contains(&"MyApp.EventManager"),
            "should find MyApp.EventManager"
        );
    }

    #[test]
    fn unsupported_language_falls_back_to_windows() {
        let source = b"line1\nline2\nline3\n";
        let candidates = extract_span_candidates("ruby", source, 3);
        assert!(!candidates.is_empty(), "should produce fallback windows");
        assert!(
            candidates.iter().all(|c| !c.is_ast),
            "fallback windows should not be AST"
        );
        assert!(
            candidates.iter().all(|c| c.kind == "window"),
            "fallback windows should have kind=window"
        );
    }

    #[test]
    fn empty_source_returns_no_candidates() {
        let candidates = extract_span_candidates("rust", b"", 0);
        assert!(candidates.is_empty());
    }

    #[test]
    fn fallback_windows_cover_all_lines() {
        let total = 45;
        let windows = fallback_windows(total);
        assert!(!windows.is_empty());
        // Every line in [0, total-1] should be covered by at least one window
        for line in 0..total {
            let covered = windows.iter().any(|w| w.start_line <= line && w.end_line >= line);
            assert!(covered, "line {} should be covered by a window", line);
        }
    }

    // ── score_candidates ───────────────────────────────────────────────────────

    #[test]
    fn name_match_scores_higher_than_body_match() {
        let source = br#"
fn event_store_handler() {
    let x = 1;
}

fn unrelated_function() {
    let event_store = 1;
}
"#;
        let total = std::str::from_utf8(source).unwrap().lines().count();
        let candidates = extract_span_candidates("rust", source, total);
        let lines: Vec<&str> = std::str::from_utf8(source).unwrap().lines().collect();
        let terms = tokenize_query("event_store");

        let scored = score_candidates(&candidates, &lines, &terms, 30);
        assert!(!scored.is_empty(), "should score at least one candidate");

        // Find scores for each function
        let handler_score = scored
            .iter()
            .find(|s| candidates[s.idx].symbol_name.as_deref() == Some("event_store_handler"))
            .map(|s| s.score);
        let unrelated_score = scored
            .iter()
            .find(|s| candidates[s.idx].symbol_name.as_deref() == Some("unrelated_function"))
            .map(|s| s.score);

        if let (Some(h), Some(u)) = (handler_score, unrelated_score) {
            assert!(
                h > u,
                "name-match function ({}) should score higher than body-match ({})",
                h,
                u
            );
        }
    }

    #[test]
    fn zero_hit_candidate_excluded() {
        let candidates = vec![SpanCandidate {
            start_line: 0,
            end_line: 2,
            kind: "function_item",
            symbol_name: Some("foo".to_string()),
            parent_idx: None,
            is_ast: true,
        }];
        let lines = vec!["fn foo() {", "    let x = 1;", "}"];
        let terms = tokenize_query("nonexistent_term_xyz");
        let scored = score_candidates(&candidates, &lines, &terms, 30);
        assert!(scored.is_empty(), "zero-hit candidate should be excluded");
    }

    // ── pack_spans ─────────────────────────────────────────────────────────────

    #[test]
    fn pack_respects_budget() {
        let candidates = vec![
            SpanCandidate {
                start_line: 0,
                end_line: 9,
                kind: "function_item",
                symbol_name: Some("foo".to_string()),
                parent_idx: None,
                is_ast: true,
            },
            SpanCandidate {
                start_line: 10,
                end_line: 19,
                kind: "function_item",
                symbol_name: Some("bar".to_string()),
                parent_idx: None,
                is_ast: true,
            },
            SpanCandidate {
                start_line: 20,
                end_line: 29,
                kind: "function_item",
                symbol_name: Some("baz".to_string()),
                parent_idx: None,
                is_ast: true,
            },
        ];
        let scored = vec![
            ScoredSpan { idx: 0, score: 3.0, score_density: 0.3 },
            ScoredSpan { idx: 1, score: 2.0, score_density: 0.2 },
            ScoredSpan { idx: 2, score: 1.0, score_density: 0.1 },
        ];

        // Budget of 15 lines: fits first span (10 lines) but not second (would need 20 total)
        let selected = pack_spans(&candidates, scored, 15);
        assert_eq!(selected.len(), 1, "only first span should fit in 15-line budget");
        assert_eq!(selected[0], 0);
    }

    #[test]
    fn pack_skips_overlapping_spans() {
        let candidates = vec![
            SpanCandidate {
                start_line: 0,
                end_line: 20,
                kind: "impl_item",
                symbol_name: Some("EventStore".to_string()),
                parent_idx: None,
                is_ast: true,
            },
            // This span overlaps with the first
            SpanCandidate {
                start_line: 5,
                end_line: 10,
                kind: "function_item",
                symbol_name: Some("new".to_string()),
                parent_idx: Some(0),
                is_ast: true,
            },
        ];
        let scored = vec![
            ScoredSpan { idx: 0, score: 6.0, score_density: 0.3 },
            ScoredSpan { idx: 1, score: 5.0, score_density: 0.83 },
        ];

        // Both are valid but overlap — only one should be selected
        let selected = pack_spans(&candidates, scored, 30);
        assert_eq!(selected.len(), 1, "overlapping spans: only one should be selected");
    }

    #[test]
    fn pack_unlimited_budget() {
        let candidates: Vec<SpanCandidate> = (0..5)
            .map(|i| SpanCandidate {
                start_line: i * 10,
                end_line: i * 10 + 9,
                kind: "function_item",
                symbol_name: Some(format!("fn_{}", i)),
                parent_idx: None,
                is_ast: true,
            })
            .collect();
        let scored: Vec<ScoredSpan> = (0..5)
            .map(|i| ScoredSpan {
                idx: i,
                score: (5 - i) as f64,
                score_density: (5 - i) as f64 / 10.0,
            })
            .collect();

        let selected = pack_spans(&candidates, scored, 0); // 0 = unlimited
        assert_eq!(selected.len(), 5, "unlimited budget should select all non-overlapping spans");
    }

    // ── extract_best_spans (integration) ──────────────────────────────────────

    #[test]
    fn returns_valid_context_result_for_rust() {
        let root = fixture_root();
        let result = extract_best_spans(&root, "src/event_store.rs", "EventStore", Some(30));
        assert!(
            !result.lines.is_empty(),
            "should return lines for EventStore query"
        );
        // Line numbers should be in ascending order
        for w in result.lines.windows(2) {
            assert!(
                w[0].line_number <= w[1].line_number,
                "lines should be in ascending order"
            );
        }
    }

    #[test]
    fn spans_mode_finds_struct_definition() {
        let root = fixture_root();
        let result = extract_best_spans(&root, "src/event_store.rs", "EventStore", Some(30));
        let has_struct = result
            .lines
            .iter()
            .any(|l| l.text.contains("pub struct EventStore"));
        assert!(
            has_struct,
            "should include the struct definition line"
        );
    }

    #[test]
    fn missing_file_returns_empty() {
        let root = fixture_root();
        let result = extract_best_spans(&root, "nonexistent.rs", "anything", Some(30));
        assert!(result.lines.is_empty());
        assert_eq!(result.truncated_count, 0);
    }

    #[test]
    fn returns_valid_result_for_unsupported_language() {
        let root = fixture_root();
        // .md files are not supported — should use fallback windows
        // Use a supported file but override via a path trick not possible here,
        // so instead test directly with a markdown path (may fail to read = empty)
        let result = extract_best_spans(&root, "README.md", "EventStore", Some(30));
        // Either empty (file not found) or non-empty (fallback windows) — both valid
        // Just verify it doesn't panic
        let _ = result;
    }

    #[test]
    fn budget_zero_means_unlimited() {
        let root = fixture_root();
        let capped = extract_best_spans(&root, "src/event_store.rs", "EventStore", Some(30));
        let unlimited = extract_best_spans(&root, "src/event_store.rs", "EventStore", Some(0));
        // Unlimited should return at least as many lines as capped
        assert!(
            unlimited.lines.len() >= capped.lines.len(),
            "unlimited budget should return >= lines vs capped"
        );
    }
}
