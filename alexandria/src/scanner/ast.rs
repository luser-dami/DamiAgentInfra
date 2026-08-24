//! Generic tree-sitter AST scanner: one engine, one node-kind spec per
//! language. Adding a language = one grammar crate + one [`LangSpec`] entry.
//! C++ keeps its own scanner (declarator unwrapping, UE macro handling);
//! everything else routes here.

use std::cell::RefCell;
use std::collections::HashMap;

use tree_sitter::{Language, Node, Parser};

use crate::model::{Edge, Symbol};

use super::LanguageScanner;
use super::common::{call_edge, import_edge, is_call_noise, make_symbol, symbol_edge};

/// Node-kind mapping for one language. All matching is by grammar node kind,
/// so a spec is pure data. NOTE: field/member-variable extraction and
/// reads/writes reference edges currently exist only in the C++ scanner
/// (the repo is 99% C++); adding them here means extending this struct with
/// field/declarator handling, not just adding spec entries.
pub(crate) struct LangSpec {
    pub tag: &'static str,
    pub language: Language,
    /// Type-definition node kinds → symbol kind (class/struct/enum/…).
    pub type_nodes: &'static [(&'static str, &'static str)],
    /// Named function/method node kinds. A node with a `body` field is a
    /// definition, without one a declaration (interface methods, TS overload
    /// signatures, trait fn prototypes).
    pub function_nodes: &'static [&'static str],
    pub import_nodes: &'static [&'static str],
    pub call_nodes: &'static [&'static str],
    /// Member-access node kinds: the callee is their leaf identifier
    /// (`obj.method()` → `method`).
    pub member_nodes: &'static [&'static str],
    /// Heritage containers under a type node (extends/base-list clauses):
    /// their identifier leaves become `inherits` edges.
    pub heritage_containers: &'static [&'static str],
}

/// Spec lookup by language tag (as produced by `language_for`). Constructed
/// per call: `Language` is `Copy`, the rest is static data.
pub(crate) fn spec_for(tag: &str) -> Option<LangSpec> {
    let spec = match tag {
        "rust" => LangSpec {
            tag: "rust",
            language: tree_sitter_rust::LANGUAGE.into(),
            type_nodes: &[
                ("struct_item", "struct"),
                ("enum_item", "enum"),
                ("trait_item", "trait"),
                ("mod_item", "module"),
            ],
            function_nodes: &["function_item"],
            import_nodes: &["use_declaration"],
            call_nodes: &["call_expression"],
            member_nodes: &["field_expression", "scoped_identifier"],
            heritage_containers: &[],
        },
        "python" => LangSpec {
            tag: "python",
            language: tree_sitter_python::LANGUAGE.into(),
            type_nodes: &[("class_definition", "class")],
            function_nodes: &["function_definition"],
            import_nodes: &["import_statement", "import_from_statement"],
            call_nodes: &["call"],
            member_nodes: &["attribute"],
            heritage_containers: &["argument_list"],
        },
        "typescript" => LangSpec {
            tag: "typescript",
            language: tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            type_nodes: &[
                ("class_declaration", "class"),
                ("interface_declaration", "interface"),
                ("enum_declaration", "enum"),
                ("type_alias_declaration", "type"),
            ],
            function_nodes: &[
                "function_declaration",
                "method_definition",
                "generator_function_declaration",
                "function_signature",
                "method_signature",
            ],
            import_nodes: &["import_statement"],
            call_nodes: &["call_expression"],
            member_nodes: &["member_expression"],
            heritage_containers: &["class_heritage"],
        },
        "tsx" => LangSpec {
            tag: "tsx",
            language: tree_sitter_typescript::LANGUAGE_TSX.into(),
            ..spec_for("typescript")?
        },
        "javascript" => LangSpec {
            tag: "javascript",
            language: tree_sitter_javascript::LANGUAGE.into(),
            type_nodes: &[("class_declaration", "class")],
            function_nodes: &[
                "function_declaration",
                "method_definition",
                "generator_function_declaration",
            ],
            import_nodes: &["import_statement"],
            call_nodes: &["call_expression"],
            member_nodes: &["member_expression"],
            heritage_containers: &["class_heritage"],
        },
        "go" => LangSpec {
            tag: "go",
            language: tree_sitter_go::LANGUAGE.into(),
            type_nodes: &[("type_declaration", "type")],
            function_nodes: &["function_declaration", "method_declaration"],
            import_nodes: &["import_declaration"],
            call_nodes: &["call_expression"],
            member_nodes: &["selector_expression"],
            heritage_containers: &[],
        },
        "java" => LangSpec {
            tag: "java",
            language: tree_sitter_java::LANGUAGE.into(),
            type_nodes: &[
                ("class_declaration", "class"),
                ("interface_declaration", "interface"),
                ("enum_declaration", "enum"),
                ("record_declaration", "record"),
            ],
            function_nodes: &["method_declaration", "constructor_declaration"],
            import_nodes: &["import_declaration"],
            call_nodes: &["method_invocation"],
            member_nodes: &[],
            heritage_containers: &["superclass", "super_interfaces"],
        },
        "csharp" => LangSpec {
            tag: "csharp",
            language: tree_sitter_c_sharp::LANGUAGE.into(),
            type_nodes: &[
                ("class_declaration", "class"),
                ("interface_declaration", "interface"),
                ("struct_declaration", "struct"),
                ("enum_declaration", "enum"),
                ("record_declaration", "record"),
                ("delegate_declaration", "delegate"),
            ],
            function_nodes: &[
                "method_declaration",
                "constructor_declaration",
                "local_function_statement",
            ],
            import_nodes: &["using_directive"],
            call_nodes: &["invocation_expression"],
            member_nodes: &["member_access_expression"],
            heritage_containers: &["base_list"],
        },
        _ => return None,
    };
    Some(spec)
}

thread_local! {
    // One parser per language per worker thread.
    static PARSERS: RefCell<HashMap<&'static str, Parser>> = RefCell::new(HashMap::new());
}

pub(crate) struct AstScanner {
    spec: LangSpec,
}

impl AstScanner {
    pub(crate) fn new(spec: LangSpec) -> Self {
        Self { spec }
    }
}

impl LanguageScanner for AstScanner {
    fn scan(&self, content: &str, file: &str) -> (Vec<Symbol>, Vec<Edge>) {
        let mut symbols = Vec::new();
        let mut edges = Vec::new();
        PARSERS.with(|cell| {
            let mut parsers = cell.borrow_mut();
            let parser = parsers
                .entry(self.spec.tag)
                .or_insert_with(|| {
                    let mut parser = Parser::new();
                    parser
                        .set_language(&self.spec.language)
                        .expect("tree-sitter grammar must load");
                    parser
                });
            let Some(tree) = parser.parse(content, None) else {
                return;
            };
            let mut stack = vec![tree.root_node()];
            while let Some(node) = stack.pop() {
                visit(node, &self.spec, content.as_bytes(), file, &mut symbols, &mut edges);
                let mut cursor = node.walk();
                stack.extend(node.children(&mut cursor));
            }
        });
        (symbols, edges)
    }
}

fn visit(
    node: Node,
    spec: &LangSpec,
    source: &[u8],
    file: &str,
    symbols: &mut Vec<Symbol>,
    edges: &mut Vec<Edge>,
) {
    let kind = node.kind();

    if let Some((_, symbol_kind)) = spec.type_nodes.iter().find(|(k, _)| *k == kind) {
        let Some(name) = symbol_name(node, source) else {
            return;
        };
        let role = if node.child_by_field_name("body").is_some() {
            "definition"
        } else {
            "declaration"
        };
        symbols.push(make_symbol(
            &name,
            symbol_kind,
            spec.tag,
            file,
            node.start_position().row + 1,
            None,
            role,
        ));
        // Inheritance: identifier leaves of the spec's heritage containers.
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if spec.heritage_containers.contains(&child.kind()) {
                let mut names = Vec::new();
                heritage_names(child, spec, source, &mut names);
                let line = child.start_position().row + 1;
                for base in names {
                    edges.push(symbol_edge(file, &name, &base, "inherits", line));
                }
            }
        }
        return;
    }

    if spec.function_nodes.contains(&kind) {
        let Some(name) = symbol_name(node, source) else {
            return;
        };
        if is_call_noise(&name) {
            return;
        }
        let role = if node.child_by_field_name("body").is_some() {
            "definition"
        } else {
            "declaration"
        };
        symbols.push(make_symbol(
            &name,
            "function",
            spec.tag,
            file,
            node.start_position().row + 1,
            Some(first_line(node, source)),
            role,
        ));
        return;
    }

    if spec.import_nodes.contains(&kind) {
        if let Some(target) = import_target(node, source) {
            edges.push(import_edge(
                file,
                &target,
                "import",
                node.start_position().row + 1,
            ));
        }
        return;
    }

    if spec.call_nodes.contains(&kind) {
        // Caller attribution: nearest enclosing *named* function; anonymous
        // scopes (arrow fns, lambdas, closures) are climbed past. Calls with
        // no enclosing function (module level) are dropped, as in C++.
        let Some(caller) = enclosing_function(node, spec, source) else {
            return;
        };
        let Some(callee) = callee_name(node, spec, source) else {
            return;
        };
        if is_call_noise(&callee) || callee == caller {
            return;
        }
        edges.push(call_edge(
            file,
            &caller,
            &callee,
            node.start_position().row + 1,
        ));
    }
}

/// Symbol name: the `name` field when the grammar provides one, else a
/// type_spec child (Go `type Foo struct {}`), else the first identifier-ish
/// direct child.
fn symbol_name(node: Node, source: &[u8]) -> Option<String> {
    if let Some(name) = node.child_by_field_name("name") {
        return Some(text(name, source).to_string());
    }
    let mut cursor = node.walk();
    node.children(&mut cursor).find_map(|child| match child.kind() {
        "identifier" | "type_identifier" | "field_identifier" | "property_identifier" => {
            Some(text(child, source).to_string())
        }
        "type_spec" => child
            .child_by_field_name("name")
            .map(|n| text(n, source).to_string()),
        _ => None,
    })
}

/// Base-type names from a heritage container: leaf-nameable children are
/// bases; clause wrappers (extends_clause, type_list, …) are descended.
fn heritage_names(node: Node, spec: &LangSpec, source: &[u8], out: &mut Vec<String>) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if let Some(name) = leaf_name(child, spec, source) {
            out.push(name);
        } else {
            heritage_names(child, spec, source, out);
        }
    }
}

/// Callee name from a call site's function slot: plain `foo()`, member
/// `obj.foo()`, scoped `ns::foo()`. Java grammars put a `name` field
/// directly on the invocation node.
fn callee_name(call: Node, spec: &LangSpec, source: &[u8]) -> Option<String> {
    if let Some(function) = call.child_by_field_name("function") {
        return leaf_name(function, spec, source);
    }
    call.child_by_field_name("name")
        .map(|n| text(n, source).to_string())
}

/// Unwrap a callee expression to its leaf identifier.
fn leaf_name(node: Node, spec: &LangSpec, source: &[u8]) -> Option<String> {
    match node.kind() {
        "identifier" | "property_identifier" | "field_identifier" | "type_identifier" => {
            Some(text(node, source).to_string())
        }
        "generic_type" | "generic_name" | "qualified_name" | "scoped_identifier" => node
            .child_by_field_name("name")
            .map(|n| text(n, source).to_string()),
        kind if spec.member_nodes.contains(&kind) => {
            for field in ["field", "property", "attribute", "name"] {
                if let Some(child) = node.child_by_field_name(field) {
                    return leaf_name(child, spec, source);
                }
            }
            let mut cursor = node.walk();
            node.named_children(&mut cursor)
                .filter_map(|child| leaf_name(child, spec, source))
                .last()
        }
        _ => None,
    }
}

/// Name of the nearest enclosing named function, climbing past anonymous
/// scopes. Lambdas attribute to their lexical owner, as in C++.
fn enclosing_function(node: Node, spec: &LangSpec, source: &[u8]) -> Option<String> {
    let mut current = node.parent();
    while let Some(ancestor) = current {
        if spec.function_nodes.contains(&ancestor.kind())
            && let Some(name) = symbol_name(ancestor, source)
        {
            return Some(name);
        }
        current = ancestor.parent();
    }
    None
}

/// Import target: a dedicated field when the grammar has one, else the first
/// path-shaped descendant, quotes/angle brackets stripped.
fn import_target(node: Node, source: &[u8]) -> Option<String> {
    for field in ["module_name", "source", "argument", "path"] {
        if let Some(child) = node.child_by_field_name(field) {
            return Some(clean_path(text(child, source)));
        }
    }
    find_descendant_kinds(
        node,
        &[
            "string_fragment",
            "interpreted_string_literal",
            "dotted_name",
            "scoped_identifier",
            "qualified_name",
            "identifier",
        ],
        source,
    )
    .map(clean_path)
}

fn find_descendant_kinds<'a>(node: Node, kinds: &[&str], source: &'a [u8]) -> Option<&'a str> {
    if kinds.contains(&node.kind()) {
        return Some(text(node, source));
    }
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find_map(|child| find_descendant_kinds(child, kinds, source))
}

fn clean_path(raw: &str) -> String {
    raw.trim()
        .trim_end_matches(';')
        .trim_matches(|c| c == '"' || c == '\'' || c == '<' || c == '>')
        .to_string()
}

fn text<'a>(node: Node, source: &'a [u8]) -> &'a str {
    std::str::from_utf8(&source[node.byte_range()]).unwrap_or("")
}

fn first_line(node: Node, source: &[u8]) -> String {
    text(node, source)
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .chars()
        .take(300)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(tag: &str, src: &str) -> (Vec<Symbol>, Vec<Edge>) {
        AstScanner::new(spec_for(tag).unwrap()).scan(src, "test.file")
    }

    #[test]
    fn python_extracts_def_class_and_import() {
        let src = "import os\nfrom pkg.mod import thing\nclass A:\n    def method(self):\n        pass\n";
        let (symbols, edges) = scan("python", src);
        assert!(symbols.iter().any(|s| s.name == "A" && s.kind == "class"));
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "method" && s.kind == "function")
        );
        assert!(edges.iter().any(|e| e.target_file == "os"));
        assert!(edges.iter().any(|e| e.target_file == "pkg.mod"));
    }

    #[test]
    fn python_attributes_calls_through_nested_functions() {
        let src = "def outer():\n    def inner():\n        helper()\n    inner()\n";
        let (_, edges) = scan("python", src);
        assert!(
            edges
                .iter()
                .any(|e| e.source_symbol == "inner" && e.target_symbol == "helper")
        );
        assert!(
            edges
                .iter()
                .any(|e| e.source_symbol == "outer" && e.target_symbol == "inner")
        );
    }

    #[test]
    fn typescript_extracts_symbols_and_import() {
        let (symbols, edges) =
            scan("typescript", "import { x } from './util';\nexport class Foo {}");
        assert!(symbols.iter().any(|s| s.name == "Foo" && s.kind == "class"));
        assert!(
            edges
                .iter()
                .any(|e| e.relation == "import" && e.target_file == "./util")
        );
    }

    #[test]
    fn typescript_extracts_call_edges_and_arrow_attribution() {
        let src = "function run() {\n  build();\n  const f = () => { emit(); };\n}\n";
        let (_, edges) = scan("typescript", src);
        assert!(
            edges
                .iter()
                .any(|e| e.source_symbol == "run" && e.target_symbol == "build")
        );
        // Arrow functions are anonymous: their calls climb to `run`.
        assert!(
            edges
                .iter()
                .any(|e| e.source_symbol == "run" && e.target_symbol == "emit")
        );
    }

    #[test]
    fn typescript_member_calls_take_leaf() {
        let src = "function run() {\n  this.service.load();\n}\n";
        let (_, edges) = scan("typescript", src);
        assert!(
            edges
                .iter()
                .any(|e| e.source_symbol == "run" && e.target_symbol == "load")
        );
    }

    #[test]
    fn typescript_overload_signature_is_declaration() {
        let src = "function f(a: number): void;\nfunction f(a: any) {}\n";
        let (symbols, _) = scan("typescript", src);
        let roles: Vec<_> = symbols.iter().map(|s| s.role.as_str()).collect();
        assert!(roles.contains(&"declaration"));
        assert!(roles.contains(&"definition"));
    }

    #[test]
    fn rust_extracts_fn_struct_use_and_calls() {
        let src = "use std::collections::HashMap;\nstruct Weapon { heat: f32 }\nimpl Weapon {\n    fn fire(&self) {\n        self.trace();\n        cooldown();\n    }\n}\nfn cooldown() {}\n";
        let (symbols, edges) = scan("rust", src);
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Weapon" && s.kind == "struct")
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "fire" && s.kind == "function" && s.role == "definition")
        );
        assert!(
            edges
                .iter()
                .any(|e| e.relation == "import" && e.target_file == "std::collections::HashMap")
        );
        assert!(
            edges
                .iter()
                .any(|e| e.source_symbol == "fire" && e.target_symbol == "trace")
        );
        assert!(
            edges
                .iter()
                .any(|e| e.source_symbol == "fire" && e.target_symbol == "cooldown")
        );
    }

    #[test]
    fn go_extracts_func_method_type_import() {
        let src = "package main\nimport \"fmt\"\ntype Weapon struct{ heat float32 }\nfunc (w Weapon) Fire() {\n    w.trace()\n    fmt.Println()\n}\n";
        let (symbols, edges) = scan("go", src);
        assert!(symbols.iter().any(|s| s.name == "Weapon" && s.kind == "type"));
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Fire" && s.kind == "function")
        );
        assert!(
            edges
                .iter()
                .any(|e| e.relation == "import" && e.target_file == "fmt")
        );
        assert!(
            edges
                .iter()
                .any(|e| e.source_symbol == "Fire" && e.target_symbol == "trace")
        );
        assert!(
            edges
                .iter()
                .any(|e| e.source_symbol == "Fire" && e.target_symbol == "Println")
        );
    }

    #[test]
    fn java_extracts_class_method_import_invocation() {
        let src = "import java.util.List;\nclass Weapon {\n    void fire() {\n        this.trace();\n    }\n}\n";
        let (symbols, edges) = scan("java", src);
        assert!(symbols.iter().any(|s| s.name == "Weapon" && s.kind == "class"));
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "fire" && s.kind == "function" && s.role == "definition")
        );
        assert!(
            edges
                .iter()
                .any(|e| e.relation == "import" && e.target_file == "java.util.List")
        );
        assert!(
            edges
                .iter()
                .any(|e| e.source_symbol == "fire" && e.target_symbol == "trace")
        );
    }

    #[test]
    fn java_interface_method_is_declaration() {
        let src = "interface Weapon {\n    void fire();\n}\n";
        let (symbols, _) = scan("java", src);
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "fire" && s.role == "declaration")
        );
    }

    #[test]
    fn csharp_extracts_class_method_using_invocation() {
        let src = "using System;\nclass Weapon {\n    void Fire() {\n        this.Trace();\n    }\n}\n";
        let (symbols, edges) = scan("csharp", src);
        assert!(symbols.iter().any(|s| s.name == "Weapon" && s.kind == "class"));
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Fire" && s.kind == "function")
        );
        assert!(
            edges
                .iter()
                .any(|e| e.relation == "import" && e.target_file == "System")
        );
        assert!(
            edges
                .iter()
                .any(|e| e.source_symbol == "Fire" && e.target_symbol == "Trace")
        );
    }
}
