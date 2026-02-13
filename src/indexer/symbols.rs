use tree_sitter::{Node, Parser};

/// Extracts symbol names (functions, structs, classes, etc.) from source code.
///
/// Returns an empty vec for unsupported languages or parse failures.
/// Symbols are returned in source order, deduplicated by name.
pub fn extract_symbols(lang: &str, source: &[u8]) -> Vec<String> {
    let symbols = match lang {
        "rust" => extract_rust(source),
        "typescript" => extract_typescript(source),
        "javascript" => extract_javascript(source),
        "python" => extract_python(source),
        "go" => extract_go(source),
        "elixir" => extract_elixir(source),
        _ => return Vec::new(),
    };

    // Deduplicate while preserving order (first occurrence wins).
    let mut seen = std::collections::HashSet::new();
    symbols
        .into_iter()
        .filter(|s| seen.insert(s.clone()))
        .collect()
}

// ── Rust ──────────────────────────────────────────────────────────────────────

fn extract_rust(source: &[u8]) -> Vec<String> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .expect("failed to load Rust grammar");
    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => return Vec::new(),
    };

    let mut symbols = Vec::new();
    walk_rust(tree.root_node(), source, &mut symbols);
    symbols
}

fn walk_rust(node: Node, source: &[u8], symbols: &mut Vec<String>) {
    match node.kind() {
        "function_item" | "function_signature_item" | "struct_item" | "enum_item"
        | "trait_item" | "const_item" | "type_item" => {
            if let Some(name) = field_name_text(&node, "name", source) {
                symbols.push(name);
            }
        }
        "impl_item" => {
            // Extract the implemented type name (e.g., "EventStore" from `impl EventStore`)
            if let Some(type_node) = node.child_by_field_name("type") {
                if let Some(name) = identifier_from_type(type_node, source) {
                    symbols.push(name);
                }
            }
        }
        _ => {}
    }

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            walk_rust(child, source, symbols);
        }
    }
}

// ── TypeScript ────────────────────────────────────────────────────────────────

fn extract_typescript(source: &[u8]) -> Vec<String> {
    let mut parser = Parser::new();
    // Use TSX parser — superset of TypeScript, handles both .ts and .tsx
    parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TSX.into())
        .expect("failed to load TypeScript grammar");
    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => return Vec::new(),
    };

    let mut symbols = Vec::new();
    walk_js_ts(tree.root_node(), source, &mut symbols, true);
    symbols
}

/// Shared walker for JavaScript and TypeScript ASTs.
///
/// When `ts_extras` is true, additionally extracts from TypeScript-specific nodes:
/// `interface_declaration`, `type_alias_declaration`, `enum_declaration`.
fn walk_js_ts(node: Node, source: &[u8], symbols: &mut Vec<String>, ts_extras: bool) {
    match node.kind() {
        "function_declaration" | "class_declaration" | "method_definition" => {
            if let Some(name) = field_name_text(&node, "name", source) {
                symbols.push(name);
            }
        }
        "interface_declaration" | "type_alias_declaration" | "enum_declaration"
            if ts_extras =>
        {
            if let Some(name) = field_name_text(&node, "name", source) {
                symbols.push(name);
            }
        }
        "variable_declarator" => {
            if is_top_level_variable(&node) {
                if let Some(name) = field_name_text(&node, "name", source) {
                    symbols.push(name);
                }
            }
        }
        _ => {}
    }

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            walk_js_ts(child, source, symbols, ts_extras);
        }
    }
}

// ── JavaScript ────────────────────────────────────────────────────────────────

fn extract_javascript(source: &[u8]) -> Vec<String> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_javascript::LANGUAGE.into())
        .expect("failed to load JavaScript grammar");
    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => return Vec::new(),
    };

    let mut symbols = Vec::new();
    walk_js_ts(tree.root_node(), source, &mut symbols, false);
    symbols
}

// ── Python ────────────────────────────────────────────────────────────────────

fn extract_python(source: &[u8]) -> Vec<String> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .expect("failed to load Python grammar");
    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => return Vec::new(),
    };

    let mut symbols = Vec::new();
    walk_python(tree.root_node(), source, &mut symbols);
    symbols
}

fn walk_python(node: Node, source: &[u8], symbols: &mut Vec<String>) {
    match node.kind() {
        "function_definition" | "class_definition" => {
            if let Some(name) = field_name_text(&node, "name", source) {
                symbols.push(name);
            }
        }
        _ => {}
    }

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            walk_python(child, source, symbols);
        }
    }
}

// ── Go ────────────────────────────────────────────────────────────────────────

fn extract_go(source: &[u8]) -> Vec<String> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_go::LANGUAGE.into())
        .expect("failed to load Go grammar");
    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => return Vec::new(),
    };

    let mut symbols = Vec::new();
    walk_go(tree.root_node(), source, &mut symbols);
    symbols
}

fn walk_go(node: Node, source: &[u8], symbols: &mut Vec<String>) {
    match node.kind() {
        "function_declaration" | "method_declaration" | "type_spec" | "const_spec" => {
            if let Some(name) = field_name_text(&node, "name", source) {
                symbols.push(name);
            }
        }
        _ => {}
    }

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            walk_go(child, source, symbols);
        }
    }
}

// ── Elixir ────────────────────────────────────────────────────────────────────

fn extract_elixir(source: &[u8]) -> Vec<String> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_elixir::LANGUAGE.into())
        .expect("failed to load Elixir grammar");
    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => return Vec::new(),
    };

    let mut symbols = Vec::new();
    walk_elixir(tree.root_node(), source, &mut symbols);
    symbols
}

fn walk_elixir(node: Node, source: &[u8], symbols: &mut Vec<String>) {
    if node.kind() == "call" {
        if let Some(id_node) = node.child_by_field_name("target") {
            if id_node.kind() == "identifier" {
                if let Ok(keyword) = id_node.utf8_text(source) {
                    match keyword {
                        "defmodule" | "defprotocol" => {
                            elixir_extract_module_name(&node, source, symbols);
                        }
                        "defimpl" => {
                            elixir_extract_impl_name(&node, source, symbols);
                        }
                        "def" | "defp" | "defmacro" | "defmacrop" | "defguard" | "defguardp"
                        | "defdelegate" => {
                            elixir_extract_fn_name(&node, source, symbols);
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            walk_elixir(child, source, symbols);
        }
    }
}

/// Extracts the first `alias` node from the arguments of a call node.
///
/// Used by `defmodule`, `defprotocol`, and `defimpl` — all follow the same
/// AST pattern: the first alias child of the `arguments` node is the name.
fn elixir_extract_first_alias(call_node: &Node, source: &[u8], symbols: &mut Vec<String>) {
    for i in 0..call_node.child_count() {
        if let Some(child) = call_node.child(i) {
            if child.kind() == "arguments" {
                for j in 0..child.named_child_count() {
                    if let Some(arg) = child.named_child(j) {
                        if arg.kind() == "alias" {
                            if let Ok(name) = arg.utf8_text(source) {
                                symbols.push(name.to_string());
                            }
                            return;
                        }
                    }
                }
                return;
            }
        }
    }
}

/// Extracts the module/protocol name from `defmodule MyApp.Accounts do ... end`.
/// The first argument is an `alias` node containing the full module name.
fn elixir_extract_module_name(call_node: &Node, source: &[u8], symbols: &mut Vec<String>) {
    elixir_extract_first_alias(call_node, source, symbols);
}

/// Extracts the protocol name from `defimpl Printable, for: Atom do ... end`.
/// The first argument is an `alias` node (the protocol being implemented).
fn elixir_extract_impl_name(call_node: &Node, source: &[u8], symbols: &mut Vec<String>) {
    elixir_extract_first_alias(call_node, source, symbols);
}

/// Extracts function/macro/guard name from def/defp/defmacro/defguard calls.
///
/// Handles three AST patterns:
/// - `def create_user(attrs)` → arguments > call > target(identifier)
/// - `def run` (no args) → arguments > identifier
/// - `def foo(x) when is_integer(x)` → arguments > binary_operator > left(call) > target(identifier)
///   (applies to any def/defp/defmacro/defguard with a `when` guard clause)
fn elixir_extract_fn_name(call_node: &Node, source: &[u8], symbols: &mut Vec<String>) {
    for i in 0..call_node.child_count() {
        if let Some(child) = call_node.child(i) {
            if child.kind() == "arguments" {
                if let Some(first_arg) = child.named_child(0) {
                    match first_arg.kind() {
                        "call" => {
                            // def func_name(args) — nested call, target is the function name
                            if let Some(fn_id) = first_arg.child_by_field_name("target") {
                                if fn_id.kind() == "identifier" {
                                    if let Ok(name) = fn_id.utf8_text(source) {
                                        symbols.push(name.to_string());
                                    }
                                }
                            }
                        }
                        "identifier" => {
                            // def func_name (no args, no parens)
                            if let Ok(name) = first_arg.utf8_text(source) {
                                symbols.push(name.to_string());
                            }
                        }
                        "binary_operator" => {
                            // defguard is_admin(user) when ... — left side is the call
                            if let Some(left) = first_arg.child_by_field_name("left") {
                                if left.kind() == "call" {
                                    if let Some(fn_id) = left.child_by_field_name("target") {
                                        if fn_id.kind() == "identifier" {
                                            if let Ok(name) = fn_id.utf8_text(source) {
                                                symbols.push(name.to_string());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                return;
            }
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Extracts the text of a named field child (typically "name").
fn field_name_text(node: &Node, field: &str, source: &[u8]) -> Option<String> {
    let child = node.child_by_field_name(field)?;
    let text = child.utf8_text(source).ok()?;
    let text = text.trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

/// Extracts the base identifier from a type node, stripping generics.
/// e.g., `Foo<T>` → "Foo", `EventStore` → "EventStore"
fn identifier_from_type(node: Node, source: &[u8]) -> Option<String> {
    match node.kind() {
        "type_identifier" => node.utf8_text(source).ok().map(|s| s.to_string()),
        "generic_type" => {
            // First child is the type identifier
            node.child(0)
                .and_then(|n| n.utf8_text(source).ok())
                .map(|s| s.to_string())
        }
        "scoped_type_identifier" => {
            // Last identifier in the path (e.g., `foo::Bar` → "Bar")
            node.child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .map(|s| s.to_string())
        }
        _ => {
            // Fallback: take the text and strip anything after '<'
            let text = node.utf8_text(source).ok()?;
            let base = text.split('<').next().unwrap_or(text).trim();
            if base.is_empty() {
                None
            } else {
                Some(base.to_string())
            }
        }
    }
}

/// Checks if a `variable_declarator` is at the top level of the module.
/// Parent chain: variable_declarator → variable_declaration → program | export_statement
fn is_top_level_variable(node: &Node) -> bool {
    let decl = match node.parent() {
        Some(p) if p.kind() == "variable_declaration" || p.kind() == "lexical_declaration" => p,
        _ => return false,
    };
    match decl.parent() {
        Some(p) => matches!(p.kind(), "program" | "export_statement"),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_extracts_all_symbol_kinds() {
        let source = br#"
pub struct Event {
    pub id: u64,
}

pub enum EventStoreError {
    NotFound,
}

pub trait Serializable {
    fn to_event(&self) -> Event;
}

pub struct EventStore {
    events: Vec<Event>,
}

impl EventStore {
    pub fn new() -> Self {
        EventStore { events: Vec::new() }
    }

    pub fn append(&mut self, event: Event) {
        self.events.push(event);
    }
}

const MAX_CAPACITY: usize = 10_000;

type EventId = u64;

fn standalone_function() {}
"#;

        let symbols = extract_symbols("rust", source);
        assert!(symbols.contains(&"Event".to_string()), "should find struct Event");
        assert!(symbols.contains(&"EventStoreError".to_string()), "should find enum");
        assert!(symbols.contains(&"Serializable".to_string()), "should find trait");
        assert!(symbols.contains(&"EventStore".to_string()), "should find struct/impl");
        assert!(symbols.contains(&"new".to_string()), "should find impl method");
        assert!(symbols.contains(&"append".to_string()), "should find impl method");
        assert!(symbols.contains(&"MAX_CAPACITY".to_string()), "should find const");
        assert!(symbols.contains(&"EventId".to_string()), "should find type alias");
        assert!(symbols.contains(&"standalone_function".to_string()), "should find fn");
        assert!(symbols.contains(&"to_event".to_string()), "should find trait method");
    }

    #[test]
    fn rust_fixture_event_store() {
        let source = include_bytes!("../../tests/fixtures/sample_repo/src/event_store.rs");
        let symbols = extract_symbols("rust", source);

        assert!(symbols.contains(&"Event".to_string()));
        assert!(symbols.contains(&"EventStoreError".to_string()));
        assert!(symbols.contains(&"Serializable".to_string()));
        assert!(symbols.contains(&"EventStore".to_string()));
        assert!(symbols.contains(&"new".to_string()));
        assert!(symbols.contains(&"append".to_string()));
        assert!(symbols.contains(&"get".to_string()));
        assert!(symbols.contains(&"count".to_string()));
        assert!(symbols.contains(&"MAX_DEFAULT_CAPACITY".to_string()));
        assert!(symbols.contains(&"EventId".to_string()));
    }

    #[test]
    fn rust_fixture_validator() {
        let source = include_bytes!("../../tests/fixtures/sample_repo/src/validator.rs");
        let symbols = extract_symbols("rust", source);

        assert!(symbols.contains(&"validate_identifier".to_string()));
        assert!(symbols.contains(&"validate_port".to_string()));
        assert!(symbols.contains(&"RuleValidator".to_string()));
        assert!(symbols.contains(&"new".to_string()));
        assert!(symbols.contains(&"add_rule".to_string()));
        assert!(symbols.contains(&"validate".to_string()));
    }

    #[test]
    fn typescript_extracts_all_symbol_kinds() {
        let source = br#"
export interface ApiRequest {
  method: string;
}

export interface ApiResponse {
  status: number;
}

export type HttpMethod = "GET" | "POST";

export enum StatusCode {
  OK = 200,
}

export class Router {
  register(handler: any): void {}
  async dispatch(req: any): Promise<any> {}
}

export function createJsonResponse(status: number): any {}

export const DEFAULT_TIMEOUT = 30000;
"#;

        let symbols = extract_symbols("typescript", source);
        assert!(symbols.contains(&"ApiRequest".to_string()), "should find interface");
        assert!(symbols.contains(&"ApiResponse".to_string()), "should find interface");
        assert!(symbols.contains(&"HttpMethod".to_string()), "should find type alias");
        assert!(symbols.contains(&"StatusCode".to_string()), "should find enum");
        assert!(symbols.contains(&"Router".to_string()), "should find class");
        assert!(symbols.contains(&"register".to_string()), "should find method");
        assert!(symbols.contains(&"dispatch".to_string()), "should find method");
        assert!(symbols.contains(&"createJsonResponse".to_string()), "should find function");
        assert!(symbols.contains(&"DEFAULT_TIMEOUT".to_string()), "should find top-level const");
    }

    #[test]
    fn typescript_fixture_handlers() {
        let source = include_bytes!("../../tests/fixtures/sample_repo/src/handlers.ts");
        let symbols = extract_symbols("typescript", source);

        assert!(symbols.contains(&"ApiRequest".to_string()));
        assert!(symbols.contains(&"ApiResponse".to_string()));
        assert!(symbols.contains(&"HttpMethod".to_string()));
        assert!(symbols.contains(&"StatusCode".to_string()));
        assert!(symbols.contains(&"RouteHandler".to_string()));
        assert!(symbols.contains(&"Router".to_string()));
        assert!(symbols.contains(&"register".to_string()));
        assert!(symbols.contains(&"dispatch".to_string()));
        assert!(symbols.contains(&"createJsonResponse".to_string()));
        assert!(symbols.contains(&"DEFAULT_TIMEOUT".to_string()));
    }

    #[test]
    fn javascript_extracts_all_symbol_kinds() {
        let source = br#"
function debounce(fn, delay) {
  return function() {};
}

class EventEmitter {
  on(event, callback) {}
  emit(event) {}
}

const MAX_RETRY_COUNT = 3;
const DEFAULT_DELAY = 1000;

module.exports = { debounce, EventEmitter };
"#;

        let symbols = extract_symbols("javascript", source);
        assert!(symbols.contains(&"debounce".to_string()), "should find function");
        assert!(symbols.contains(&"EventEmitter".to_string()), "should find class");
        assert!(symbols.contains(&"on".to_string()), "should find method");
        assert!(symbols.contains(&"emit".to_string()), "should find method");
        assert!(symbols.contains(&"MAX_RETRY_COUNT".to_string()), "should find top-level const");
        assert!(symbols.contains(&"DEFAULT_DELAY".to_string()), "should find top-level const");
    }

    #[test]
    fn javascript_fixture_utils() {
        let source = include_bytes!("../../tests/fixtures/sample_repo/src/utils.js");
        let symbols = extract_symbols("javascript", source);

        assert!(symbols.contains(&"debounce".to_string()));
        assert!(symbols.contains(&"throttle".to_string()));
        assert!(symbols.contains(&"EventEmitter".to_string()));
        assert!(symbols.contains(&"deepClone".to_string()));
        assert!(symbols.contains(&"MAX_RETRY_COUNT".to_string()));
        assert!(symbols.contains(&"DEFAULT_DELAY".to_string()));
    }

    #[test]
    fn python_extracts_all_symbol_kinds() {
        let source = br#"
from dataclasses import dataclass

@dataclass
class User:
    id: int
    username: str

    def has_role(self, role):
        return role in self.roles

class UserRepository:
    def save(self, user):
        pass

    def find_by_id(self, user_id):
        pass

def create_default_admin():
    pass

def validate_email(email):
    pass
"#;

        let symbols = extract_symbols("python", source);
        assert!(symbols.contains(&"User".to_string()), "should find decorated class");
        assert!(symbols.contains(&"has_role".to_string()), "should find method");
        assert!(symbols.contains(&"UserRepository".to_string()), "should find class");
        assert!(symbols.contains(&"save".to_string()), "should find method");
        assert!(symbols.contains(&"find_by_id".to_string()), "should find method");
        assert!(symbols.contains(&"create_default_admin".to_string()), "should find function");
        assert!(symbols.contains(&"validate_email".to_string()), "should find function");
    }

    #[test]
    fn python_fixture_models() {
        let source = include_bytes!("../../tests/fixtures/sample_repo/src/models.py");
        let symbols = extract_symbols("python", source);

        assert!(symbols.contains(&"User".to_string()));
        assert!(symbols.contains(&"UserRepository".to_string()));
        assert!(symbols.contains(&"Permission".to_string()));
        assert!(symbols.contains(&"create_default_admin".to_string()));
        assert!(symbols.contains(&"validate_email".to_string()));
    }

    #[test]
    fn go_extracts_all_symbol_kinds() {
        let source = br#"
package main

type ServerConfig struct {
	Host string
	Port int
}

type Server struct {
	config ServerConfig
}

func NewServer(config ServerConfig) *Server {
	return &Server{config: config}
}

func (s *Server) Start() error {
	return nil
}

func HealthCheck() {}

const DefaultPort = 8080
const MaxRequestSize = 1048576
"#;

        let symbols = extract_symbols("go", source);
        assert!(symbols.contains(&"ServerConfig".to_string()), "should find type");
        assert!(symbols.contains(&"Server".to_string()), "should find type");
        assert!(symbols.contains(&"NewServer".to_string()), "should find function");
        assert!(symbols.contains(&"Start".to_string()), "should find method");
        assert!(symbols.contains(&"HealthCheck".to_string()), "should find function");
        assert!(symbols.contains(&"DefaultPort".to_string()), "should find const");
        assert!(symbols.contains(&"MaxRequestSize".to_string()), "should find const");
    }

    #[test]
    fn go_fixture_server() {
        let source = include_bytes!("../../tests/fixtures/sample_repo/src/server.go");
        let symbols = extract_symbols("go", source);

        assert!(symbols.contains(&"ServerConfig".to_string()));
        assert!(symbols.contains(&"Server".to_string()));
        assert!(symbols.contains(&"NewServer".to_string()));
        assert!(symbols.contains(&"Start".to_string()));
        assert!(symbols.contains(&"IsRunning".to_string()));
        assert!(symbols.contains(&"HealthCheck".to_string()));
        assert!(symbols.contains(&"DefaultPort".to_string()));
        assert!(symbols.contains(&"MaxRequestSize".to_string()));
    }

    #[test]
    fn elixir_extracts_all_symbol_kinds() {
        let source = br#"
defmodule MyApp.Accounts do
  defstruct [:name, :email]

  def create_user(attrs) do
    attrs
  end

  defp validate(changeset) do
    changeset
  end

  defmacro log_call(func_name) do
    quote do
      IO.puts(unquote(func_name))
    end
  end

  defmacrop internal_helper(x) do
    x
  end

  defguard is_admin(user) when user.role == :admin

  defguardp is_positive(x) when x > 0

  def run do
    :ok
  end
end

defprotocol Printable do
  def to_string(data)
end

defimpl Printable, for: Atom do
  def to_string(atom), do: Atom.to_string(atom)
end

defmodule MyApp.Accounts.Permissions do
  defdelegate fetch(key), to: Map
end
"#;

        let symbols = extract_symbols("elixir", source);
        assert!(symbols.contains(&"MyApp.Accounts".to_string()), "should find defmodule");
        assert!(symbols.contains(&"create_user".to_string()), "should find def");
        assert!(symbols.contains(&"validate".to_string()), "should find defp");
        assert!(symbols.contains(&"log_call".to_string()), "should find defmacro");
        assert!(symbols.contains(&"internal_helper".to_string()), "should find defmacrop");
        assert!(symbols.contains(&"is_admin".to_string()), "should find defguard");
        assert!(symbols.contains(&"is_positive".to_string()), "should find defguardp");
        assert!(symbols.contains(&"run".to_string()), "should find no-arg def");
        assert!(symbols.contains(&"Printable".to_string()), "should find defprotocol");
        assert!(symbols.contains(&"MyApp.Accounts.Permissions".to_string()), "should find nested module");
        assert!(symbols.contains(&"fetch".to_string()), "should find defdelegate");
        // defimpl extracts the protocol name being implemented
        assert!(symbols.iter().filter(|s| *s == "Printable").count() >= 1, "should find defimpl protocol name");
    }

    #[test]
    fn elixir_fixture_event_manager() {
        let source = include_bytes!("../../tests/fixtures/sample_repo/src/event_manager.ex");
        let symbols = extract_symbols("elixir", source);

        assert!(symbols.contains(&"MyApp.EventManager".to_string()));
        assert!(symbols.contains(&"new".to_string()));
        assert!(symbols.contains(&"subscribe".to_string()));
        assert!(symbols.contains(&"dispatch_to_handler".to_string()));
        assert!(symbols.contains(&"log_event".to_string()));
        assert!(symbols.contains(&"defevent".to_string()));
        assert!(symbols.contains(&"validate_handler".to_string()));
        assert!(symbols.contains(&"is_event".to_string()));
        assert!(symbols.contains(&"is_valid_name".to_string()));
        assert!(symbols.contains(&"run".to_string()));
        assert!(symbols.contains(&"MyApp.EventManager.Supervisor".to_string()));
        assert!(symbols.contains(&"start_link".to_string()));
        assert!(symbols.contains(&"MyApp.Publishable".to_string()));
        assert!(symbols.contains(&"to_event".to_string()));
        assert!(symbols.contains(&"MyApp.EventManager.Delegate".to_string()));
        assert!(symbols.contains(&"fetch".to_string()));
    }

    #[test]
    fn elixir_nested_modules() {
        let source = br#"
defmodule MyApp.Outer do
  defmodule Inner do
    def hello, do: :world
  end

  def greet, do: :hi
end
"#;

        let symbols = extract_symbols("elixir", source);
        assert!(symbols.contains(&"MyApp.Outer".to_string()), "should find outer module");
        assert!(symbols.contains(&"Inner".to_string()), "should find nested module");
        assert!(symbols.contains(&"hello".to_string()), "should find inner function");
        assert!(symbols.contains(&"greet".to_string()), "should find outer function");
    }

    #[test]
    fn elixir_no_arg_function() {
        let source = br#"
defmodule M do
  def run do
    :ok
  end

  defp setup do
    :ready
  end
end
"#;

        let symbols = extract_symbols("elixir", source);
        assert!(symbols.contains(&"run".to_string()), "should find no-arg def");
        assert!(symbols.contains(&"setup".to_string()), "should find no-arg defp");
    }

    #[test]
    fn elixir_guarded_function() {
        let source = br#"
defmodule M do
  defguard is_admin(user) when user.role == :admin
  defguardp is_active(user) when user.active == true
end
"#;

        let symbols = extract_symbols("elixir", source);
        assert!(symbols.contains(&"is_admin".to_string()), "should find defguard");
        assert!(symbols.contains(&"is_active".to_string()), "should find defguardp");
    }

    #[test]
    fn elixir_multi_clause_functions_deduplicate() {
        let source = br#"
defmodule M do
  def process(:add, x, y), do: x + y
  def process(:sub, x, y), do: x - y
end
"#;

        let symbols = extract_symbols("elixir", source);
        let count = symbols.iter().filter(|s| *s == "process").count();
        assert_eq!(count, 1, "multi-clause function 'process' should appear exactly once after dedup");
    }

    #[test]
    fn elixir_pattern_matching_in_function_heads() {
        let source = br#"
defmodule M do
  def handle_call({:get, key}, _from, state) do
    {:reply, Map.get(state, key), state}
  end
end
"#;

        let symbols = extract_symbols("elixir", source);
        assert!(symbols.contains(&"handle_call".to_string()), "should extract handle_call with pattern-matched args");
    }

    #[test]
    fn elixir_default_arguments() {
        let source = br#"
defmodule M do
  def connect(host, port \\ 4000) do
    {host, port}
  end
end
"#;

        let symbols = extract_symbols("elixir", source);
        assert!(symbols.contains(&"connect".to_string()), "should extract connect with default argument");
    }

    #[test]
    fn elixir_callback_and_spec_not_extracted() {
        let source = br#"
defmodule MyBehaviour do
  @callback init(opts :: keyword()) :: {:ok, term()}
  @spec validate(term()) :: boolean()
  def validate(x), do: is_map(x)
end
"#;

        let symbols = extract_symbols("elixir", source);
        assert!(symbols.contains(&"validate".to_string()), "should extract def validate");
        assert!(!symbols.contains(&"init".to_string()), "@callback should NOT produce a symbol");
        assert!(!symbols.contains(&"callback".to_string()), "'callback' should NOT be a symbol");
        assert!(!symbols.contains(&"spec".to_string()), "'spec' should NOT be a symbol");
    }

    #[test]
    fn elixir_defstruct_not_standalone_symbol() {
        let source = br#"
defmodule User do
  defstruct [:name, :email]
end
"#;

        let symbols = extract_symbols("elixir", source);
        assert!(symbols.contains(&"User".to_string()), "should extract module User");
        assert!(!symbols.contains(&"defstruct".to_string()), "defstruct should NOT produce a standalone symbol");
    }

    #[test]
    fn unsupported_language_returns_empty() {
        let symbols = extract_symbols("ruby", b"class Foo; end");
        assert!(symbols.is_empty());
    }

    #[test]
    fn empty_source_returns_empty() {
        let symbols = extract_symbols("rust", b"");
        assert!(symbols.is_empty());
    }

    #[test]
    fn deduplicates_symbols() {
        // EventStore appears both as struct and impl type
        let source = br#"
struct EventStore {}
impl EventStore {
    fn new() -> Self { EventStore {} }
}
"#;
        let symbols = extract_symbols("rust", source);
        let event_store_count = symbols.iter().filter(|s| *s == "EventStore").count();
        assert_eq!(event_store_count, 1, "EventStore should appear only once after dedup");
    }

    #[test]
    fn js_ignores_nested_variables() {
        let source = br#"
const TOP_LEVEL = 1;

function foo() {
    const nested = 2;
    let inner = 3;
}
"#;
        let symbols = extract_symbols("javascript", source);
        assert!(symbols.contains(&"TOP_LEVEL".to_string()), "should find top-level const");
        assert!(symbols.contains(&"foo".to_string()), "should find function");
        assert!(!symbols.contains(&"nested".to_string()), "should NOT find nested const");
        assert!(!symbols.contains(&"inner".to_string()), "should NOT find nested let");
    }

    #[test]
    fn js_extracts_arrow_and_function_expressions() {
        let source = br#"
const fetchUser = async (id) => {
    return db.get(id);
};

const handleRequest = function(req) {
    return process(req);
};

const middleware = (ctx) => ctx.next();

function outer() {
    const nestedArrow = () => {};
    const nestedFn = function() {};
}
"#;
        let symbols = extract_symbols("javascript", source);
        assert!(symbols.contains(&"fetchUser".to_string()), "should find top-level arrow fn");
        assert!(symbols.contains(&"handleRequest".to_string()), "should find top-level fn expression");
        assert!(symbols.contains(&"middleware".to_string()), "should find top-level concise arrow");
        assert!(symbols.contains(&"outer".to_string()), "should find function declaration");
        assert!(!symbols.contains(&"nestedArrow".to_string()), "should NOT find nested arrow");
        assert!(!symbols.contains(&"nestedFn".to_string()), "should NOT find nested fn expression");
    }

    #[test]
    fn ts_extracts_arrow_and_function_expressions() {
        let source = br#"
export const fetchUser = async (id: string): Promise<User> => {
    return db.get(id);
};

export const handleRequest = function(req: Request): Response {
    return process(req);
};

const middleware = (ctx: Context) => ctx.next();
"#;
        let symbols = extract_symbols("typescript", source);
        assert!(symbols.contains(&"fetchUser".to_string()), "should find exported arrow fn");
        assert!(symbols.contains(&"handleRequest".to_string()), "should find exported fn expression");
        assert!(symbols.contains(&"middleware".to_string()), "should find top-level arrow");
    }
}
