//! C++ AST scanner: tree-sitter parse, then symbols (classes, functions with
//! definition/declaration roles) and edges (`#include`, caller-attributed
//! calls) extracted from the syntax tree. Unlike the old regex scanner this
//! is comment/string-safe, handles multi-line signatures, templates, and
//! attributs calls to their enclosing function via AST ancestry — no
//! brace-depth guessing.

use std::cell::RefCell;

use regex::Regex;
use std::sync::LazyLock;
use tree_sitter::{Node, Parser};

use crate::model::{Edge, Symbol};

use super::LanguageScanner;
use super::common::{call_edge, import_edge, is_call_noise, make_symbol, symbol_edge};

thread_local! {
    // One parser per worker thread; set_language is cheap but not free, and a
    // scan touches thousands of files.
    static PARSER: RefCell<Parser> = RefCell::new({
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_cpp::LANGUAGE.into())
            .expect("tree-sitter-cpp grammar must load");
        parser
    });
}

/// UE export macros between `class`/`struct` and the type name
/// (`class LYRAGAME_API UWeapon`) derail the grammar's error recovery — the
/// base clause and body get swallowed into ERROR nodes. Strip the ALL-CAPS
/// token inline before parsing; only same-line bytes are removed, so every
/// row number in the parse tree still matches the original file. The macro
/// must be followed by a name token: an all-caps *class name* (`class
/// FJSONValue {`) is not a macro and must survive (the `regex` crate has no
/// lookahead, so the following word is captured and checked in the closure).
static API_MACRO: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(\b(?:class|struct)\s+)[A-Z][A-Z0-9_]+\s+([A-Za-z_]\w*)").unwrap()
});

/// Terminate standalone UE reflection macro invocations (`UCLASS(...)`,
/// `USTRUCT(...)`, `UENUM(...)`, `GENERATED_BODY()`) with a trailing `;`.
/// These macros are written without semicolons in UE headers; tree-sitter
/// then keeps the unterminated declaration open and error-recovery can
/// swallow the following `class`/`struct` — or, when GENERATED_BODY() is the
/// only class-body member, the whole class — into an ERROR node. The added
/// `;` sits at the line end, so row numbers are unchanged. Multi-line macro
/// calls are left untouched.
fn terminate_ue_macros(content: &str) -> String {
    let mut out = String::with_capacity(content.len() + 128);
    for line in content.split('\n') {
        let trimmed = line.trim_end_matches('\r').trim_start();
        let needs = (trimmed.starts_with("UCLASS(")
            || trimmed.starts_with("USTRUCT(")
            || trimmed.starts_with("UENUM(")
            || trimmed.starts_with("GENERATED_BODY("))
            && trimmed.ends_with(')');
        out.push_str(line);
        if needs {
            out.push(';');
        }
        out.push('\n');
    }
    out
}

/// Strip `class MACRO Name` → `class Name`, keeping everything else intact.
fn strip_api_macros(content: &str) -> std::borrow::Cow<'_, str> {
    API_MACRO.replace_all(content, |caps: &regex::Captures| {
        // `class FJSONValue final` — the all-caps token IS the class name.
        if &caps[2] == "final" {
            caps[0].to_string()
        } else {
            format!("{}{}", &caps[1], &caps[2])
        }
    })
}
pub(crate) struct CppScanner;

impl LanguageScanner for CppScanner {
    fn scan(&self, content: &str, file: &str) -> (Vec<Symbol>, Vec<Edge>) {
        let terminated = terminate_ue_macros(content);
        let cleaned = strip_api_macros(&terminated);
        let mut ctx = ScanCtx {
            source: cleaned.as_bytes(),
            file,
            symbols: Vec::new(),
            edges: Vec::new(),
            locals: std::collections::HashMap::new(),
        };
        PARSER.with(|cell| {
            let tree = match cell.borrow_mut().parse(&*cleaned, None) {
                Some(tree) => tree,
                None => return,
            };
            let mut stack = vec![tree.root_node()];
            while let Some(node) = stack.pop() {
                visit(node, &mut ctx);
                let mut cursor = node.walk();
                stack.extend(node.children(&mut cursor));
            }
        });
        let ScanCtx { symbols, mut edges, .. } = ctx;
        // Type/inheritance edges repeat per parameter; keep the first site.
        // Variable references keep their line — every read/write site matters.
        let mut seen = std::collections::HashSet::new();
        edges.retain(|e| {
            if e.relation == "call" || e.relation == "include" {
                return true;
            }
            let line = if e.relation == "reads" || e.relation == "writes" {
                e.line
            } else {
                0
            };
            seen.insert((
                e.source_symbol.clone(),
                e.target_symbol.clone(),
                e.relation.clone(),
                line,
            ))
        });
        (symbols, edges)
    }
}

/// Per-file extraction state threaded through the AST walk.
struct ScanCtx<'a> {
    source: &'a [u8],
    file: &'a str,
    symbols: Vec<Symbol>,
    edges: Vec<Edge>,
    /// Memo of local variable names per function node, for shadow filtering.
    locals: std::collections::HashMap<usize, std::collections::HashSet<String>>,
}

fn visit(node: Node, ctx: &mut ScanCtx) {
    let source = ctx.source;
    let file = ctx.file;
    match node.kind() {
        // Both `class` and `struct` parse as *_specifier.
        kind @ ("class_specifier" | "struct_specifier") => {
            // No body => forward declaration (`class Foo;`) — declares a name
            // but points nowhere; must not shadow the real definition.
            if node.child_by_field_name("body").is_none() {
                return;
            }
            let Some(name) = class_name(node, source) else {
                return;
            };
            ctx.symbols.push(make_symbol(
                &name,
                kind.strip_suffix("_specifier").unwrap_or("class"),
                "cpp",
                file,
                node.start_position().row + 1,
                None,
                "definition",
            ));
            // Inheritance: `class A : public B, public I` → A inherits B, I.
            if let Some(bases) = child_of_kind(node, "base_class_clause") {
                let line = bases.start_position().row + 1;
                for base in type_names_in(bases, source) {
                    ctx.edges.push(symbol_edge(file, &name, &base, "inherits", line));
                }
            }
        }
        "function_definition" => {
            // A real function head unwraps to a function_declarator; anything
            // else is grammar error recovery (e.g. a mangled class head) and
            // must not become a bogus function symbol.
            let Some(head) = node
                .child_by_field_name("declarator")
                .and_then(|declarator| find_descendant(declarator, "function_declarator"))
            else {
                return;
            };
            let Some((name, qualified)) = function_name(head, source) else {
                return;
            };
            let qualified = qualify_with_enclosing_class(node, qualified, source);
            let mut symbol = make_symbol(
                &name,
                "function",
                "cpp",
                file,
                node.start_position().row + 1,
                Some(headline(node, source)),
                "definition",
            );
            symbol.qualified_name = qualified;
            ctx.symbols.push(symbol);
            collect_type_uses(node, head, &name, file, source, &mut ctx.edges);
        }
        // Prototypes at file/namespace scope (`declaration`) and inside class
        // bodies (`field_declaration`): both are recorded, tagged — resolution
        // prefers the definition, clangd-style. A field_declaration without a
        // function declarator is a member variable (UPROPERTYs land here).
        "declaration" | "field_declaration" => {
            if let Some(declarator) = find_descendant(node, "function_declarator") {
                let Some((name, qualified)) = function_name(declarator, source) else {
                    return;
                };
                if is_call_noise(&name) {
                    return;
                }
                // No type field means the "declaration" is either a constructor/
                // destructor prototype (`UWeapon();`) or a macro the grammar
                // recovered into declarator shape (`GENERATED_BODY()`). Keep only
                // the former: name must match the enclosing class.
                if node.child_by_field_name("type").is_none()
                    && !is_ctor_dtor_of_enclosing_class(node, &name, source)
                {
                    return;
                }
                let qualified = qualify_with_enclosing_class(node, qualified, source);
                let mut symbol = make_symbol(
                    &name,
                    "function",
                    "cpp",
                    file,
                    node.start_position().row + 1,
                    Some(first_line(node, source)),
                    "declaration",
                );
                symbol.qualified_name = qualified;
                ctx.symbols.push(symbol);
                collect_type_uses(node, declarator, &name, file, source, &mut ctx.edges);
                return;
            }
            if node.kind() != "field_declaration" {
                return;
            }
            // Member variable: name from the declarator, type-usage edges from
            // the declared type (`ULyraWeaponInstance* Weapon` → uses_type).
            let Some(declarator) = node.child_by_field_name("declarator") else {
                return;
            };
            let Some(name) = declarator_name(declarator, source) else {
                return;
            };
            if is_call_noise(&name) {
                return;
            }
            let qualified = qualify_with_enclosing_class(node, name.clone(), source);
            let mut symbol = make_symbol(
                &name,
                "field",
                "cpp",
                file,
                node.start_position().row + 1,
                Some(first_line(node, source)),
                "definition",
            );
            symbol.qualified_name = qualified;
            ctx.symbols.push(symbol);
            if let Some(ty) = node.child_by_field_name("type") {
                let line = ty.start_position().row + 1;
                for used in type_names_in(ty, source) {
                    ctx.edges.push(symbol_edge(file, &name, &used, "uses_type", line));
                }
            }
        }
        "preproc_include" => {
            let Some(path) = node
                .child_by_field_name("path")
                .map(|p| &source[p.byte_range()])
                .and_then(|raw| std::str::from_utf8(raw).ok())
            else {
                return;
            };
            let target = path.trim_matches(|c| c == '"' || c == '<' || c == '>');
            ctx.edges.push(import_edge(
                file,
                target,
                "include",
                node.start_position().row + 1,
            ));
        }
        "call_expression" => {
            // Caller attribution: nearest enclosing function_definition. Calls
            // at global scope (static init, macros like UCLASS/GENERATED_BODY)
            // have no caller and are dropped — same contract as before.
            let Some(caller) = enclosing_function(node, source) else {
                return;
            };
            let Some(callee) = callee_name(node, source) else {
                return;
            };
            if is_call_noise(&callee) || callee == caller {
                return;
            }
            ctx.edges.push(call_edge(
                file,
                &caller,
                &callee,
                node.start_position().row + 1,
            ));
        }
        // Candidate variable reference: any identifier used inside a function
        // body that is not a declaration, callee, or scoped name. Resolution
        // (same-file → class scope → global unique) decides later whether it
        // names a field; unresolved rows are the candidate set.
        "identifier" | "field_identifier" => {
            if is_non_reference_position(node) {
                return;
            }
            let Some(function) = enclosing_function_node(node) else {
                return;
            };
            let name = text(node, source);
            if is_call_noise(name) {
                return;
            }
            // Shadow filter: a local variable or parameter with this name
            // makes every use local, never a field reference.
            let local_names = ctx.locals
                .entry(function.id())
                .or_insert_with(|| collect_locals(function, source));
            if local_names.contains(name) {
                return;
            }
            let Some(caller) = function
                .child_by_field_name("declarator")
                .and_then(|d| find_descendant(d, "function_declarator"))
                .and_then(|head| function_name(head, source))
                .map(|(name, _)| name)
            else {
                return;
            };
            if name == caller {
                return;
            }
            let relation = if is_write_position(node) {
                "writes"
            } else {
                "reads"
            };
            ctx.edges.push(symbol_edge(
                file,
                &caller,
                name,
                relation,
                node.start_position().row + 1,
            ));
        }
        _ => {}
    }
}

/// Class name: take the *last* direct type_identifier child, so a stray
/// macro-shaped identifier the grammar recovered from can never shadow the
/// real name (base classes live under base_class_clause, not direct children).
fn class_name(node: Node, source: &[u8]) -> Option<String> {
    let mut name = None;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "type_identifier" {
            name = Some(child);
        }
    }
    name.map(|n| text(n, source).to_string())
}

/// Unwrap a declarator down to the function's name. Returns
/// `(name, qualified_name)`; for out-of-line definitions the qualified form
/// keeps the `Class::` prefix as the weak declaration↔definition fingerprint.
fn function_name(declarator: Node, source: &[u8]) -> Option<(String, String)> {
    match declarator.kind() {
        "function_declarator" | "pointer_declarator" | "reference_declarator"
        | "parenthesized_declarator" => {
            function_name(declarator.child_by_field_name("declarator")?, source)
        }
        "identifier" | "field_identifier" | "type_identifier" | "destructor_name"
        | "operator_name" => {
            let name = text(declarator, source).to_string();
            Some((name.clone(), name))
        }
        "qualified_identifier" => {
            let qualified = text(declarator, source).to_string();
            let name = declarator
                .child_by_field_name("name")
                .map(|n| text(n, source).to_string())
                .unwrap_or_else(|| qualified.clone());
            Some((name, qualified))
        }
        "template_function" => {
            let name = declarator
                .child_by_field_name("name")
                .map(|n| text(n, source).to_string())?;
            Some((name.clone(), name))
        }
        _ => None,
    }
}

/// Callee name from a call site's function slot: plain `foo()`, scoped
/// `ns::foo()`, member `obj.foo()` / `ptr->foo()`, and `foo<T>()`.
fn callee_name(call: Node, source: &[u8]) -> Option<String> {
    let function = call.child_by_field_name("function")?;
    match function.kind() {
        "identifier" => Some(text(function, source).to_string()),
        "qualified_identifier" => function
            .child_by_field_name("name")
            .map(|n| text(n, source).to_string()),
        "field_expression" => function
            .child_by_field_name("field")
            .map(|n| text(n, source).to_string()),
        "template_function" => function
            .child_by_field_name("name")
            .map(|n| text(n, source).to_string()),
        _ => None,
    }
}

/// Name of the nearest enclosing function definition (walks through lambdas,
/// which are not function_definitions, attributing their calls to the outer
/// function — the closest lexical owner).
fn enclosing_function(node: Node, source: &[u8]) -> Option<String> {
    let function = enclosing_function_node(node)?;
    let declarator = function.child_by_field_name("declarator")?;
    let head = find_descendant(declarator, "function_declarator")?;
    function_name(head, source).map(|(name, _)| name)
}

/// The nearest enclosing function_definition node, if any.
fn enclosing_function_node(node: Node) -> Option<Node> {
    let mut current = node.parent();
    while let Some(ancestor) = current {
        if ancestor.kind() == "function_definition" {
            return Some(ancestor);
        }
        current = ancestor.parent();
    }
    None
}

/// True when the identifier sits in a position that is not a variable read:
/// declarator names, parameter names, callee leaves, scoped names, labels.
fn is_non_reference_position(node: Node) -> bool {
    let Some(parent) = node.parent() else {
        return true;
    };
    match parent.kind() {
        "function_declarator" | "parameter_declaration" | "template_function"
        | "template_method" | "labeled_statement" | "goto_statement"
        | "template_argument_list" | "qualified_identifier" => true,
        "declaration" | "init_declarator" | "pointer_declarator" | "reference_declarator"
        | "array_declarator" | "parenthesized_declarator" => {
            parent.child_by_field_name("declarator") == Some(node)
        }
        "call_expression" => parent.child_by_field_name("function") == Some(node),
        // Member access: a variable read (`obj.Field`), unless the whole
        // access is the callee of a call (`obj.Fire()` — the call edge
        // already covers it).
        "field_expression" | "parenthesized_expression" => parent.parent().is_some_and(
            |grandparent| {
                grandparent.kind() == "call_expression"
                    && grandparent.child_by_field_name("function") == Some(parent)
            },
        ),
        _ => false,
    }
}

/// True when the identifier is being written: the left side of an assignment
/// (`=`, `+=`, …) or the operand of `++`/`--`, looking through member access
/// and parentheses (`this->Field = x`, `(Count)++`).
fn is_write_position(node: Node) -> bool {
    let mut current = node;
    loop {
        let Some(parent) = current.parent() else {
            return false;
        };
        match parent.kind() {
            "assignment_expression" => {
                return parent.child_by_field_name("left") == Some(current);
            }
            "update_expression" => return true,
            "field_expression" | "parenthesized_expression" | "subscript_expression"
            | "pointer_expression" | "unary_expression" => current = parent,
            _ => return false,
        }
    }
}

/// Names declared locally in a function (parameters + local declarations),
/// for shadow filtering: a match means the identifier is local, never a
/// field reference. Nested-lambda locals are unioned in — a deliberate
/// over-approximation that can only drop references, never invent them.
fn collect_locals(function: Node, source: &[u8]) -> std::collections::HashSet<String> {
    let mut names = std::collections::HashSet::new();
    let mut stack = vec![function];
    while let Some(node) = stack.pop() {
        if matches!(node.kind(), "declaration" | "parameter_declaration")
            && let Some(declarator) = node.child_by_field_name("declarator")
            && let Some(name) = declarator_name(declarator, source)
        {
            names.insert(name);
        }
        let mut cursor = node.walk();
        stack.extend(node.children(&mut cursor));
    }
    names
}

/// Variable name from a declarator, unwrapping pointer/array/initialiser
/// layers (`ULyraWeaponInstance* Weapon = nullptr` → `Weapon`).
fn declarator_name(declarator: Node, source: &[u8]) -> Option<String> {
    match declarator.kind() {
        "identifier" | "field_identifier" => Some(text(declarator, source).to_string()),
        "pointer_declarator" | "reference_declarator" | "array_declarator"
        | "parenthesized_declarator" | "init_declarator" => {
            declarator_name(declarator.child_by_field_name("declarator")?, source)
        }
        _ => None,
    }
}

/// Leaf type names referenced by a type subtree: `ULyraWeaponInstance*`,
/// `TArray<FHeatState>`, `TSubclassOf<UWeapon>` each yield their identifier
/// leaves (template name and arguments). `primitive_type` is a distinct
/// grammar kind, so int/float/bool never match — excluded for free.
fn type_names_in(node: Node, source: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    let mut stack = vec![node];
    while let Some(current) = stack.pop() {
        match current.kind() {
            "type_identifier" => {
                let name = text(current, source);
                // Fixed-width typedefs parse as identifiers but carry no
                // project-type information.
                if !matches!(
                    name,
                    "int8" | "int16" | "int32" | "int64" | "uint8" | "uint16" | "uint32"
                        | "uint64" | "size_t" | "intptr_t" | "uintptr_t" | "ptrdiff_t"
                        | "PTRINT" | "UPTRINT" | "TCHAR"
                ) {
                    names.push(name.to_string());
                }
            }
            "qualified_identifier" => {
                if let Some(name) = current.child_by_field_name("name") {
                    names.push(text(name, source).to_string());
                }
            }
            _ => {
                let mut cursor = current.walk();
                stack.extend(current.children(&mut cursor));
            }
        }
    }
    names
}

/// Emit `uses_type` edges for a function's return type and parameter types.
fn collect_type_uses(
    declaration: Node,
    head: Node,
    name: &str,
    file: &str,
    source: &[u8],
    edges: &mut Vec<Edge>,
) {
    if let Some(ret) = declaration.child_by_field_name("type") {
        let line = ret.start_position().row + 1;
        for used in type_names_in(ret, source) {
            edges.push(symbol_edge(file, name, &used, "uses_type", line));
        }
    }
    if let Some(params) = head.child_by_field_name("parameters") {
        let mut cursor = params.walk();
        for param in params.children(&mut cursor) {
            if param.kind() != "parameter_declaration" {
                continue;
            }
            let Some(ty) = param.child_by_field_name("type") else {
                continue;
            };
            let line = ty.start_position().row + 1;
            for used in type_names_in(ty, source) {
                edges.push(symbol_edge(file, name, &used, "uses_type", line));
            }
        }
    }
}

fn child_of_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    node.children(&mut cursor).find(|child| child.kind() == kind)
}

/// True when `name` is the constructor or destructor of the nearest
/// enclosing class (`UWeapon` / `~UWeapon` inside `class UWeapon`).
fn is_ctor_dtor_of_enclosing_class(node: Node, name: &str, source: &[u8]) -> bool {
    let mut current = node.parent();
    while let Some(ancestor) = current {
        if matches!(ancestor.kind(), "class_specifier" | "struct_specifier") {
            let Some(class) = class_name(ancestor, source) else {
                return false;
            };
            return name == class || name == format!("~{class}");
        }
        current = ancestor.parent();
    }
    false
}

/// Give an unqualified in-class function head its `Class::` prefix, so a
/// member definition/declaration shares one qualified fingerprint with its
/// out-of-line counterpart. Climb stops at function boundaries: a local
/// function is not a member of the outer function's class.
fn qualify_with_enclosing_class(node: Node, qualified: String, source: &[u8]) -> String {
    if qualified.contains("::") {
        return qualified;
    }
    let mut current = node.parent();
    while let Some(ancestor) = current {
        match ancestor.kind() {
            "function_definition" => break,
            "class_specifier" | "struct_specifier" => {
                if let Some(class) = class_name(ancestor, source) {
                    return format!("{class}::{qualified}");
                }
            }
            _ => {}
        }
        current = ancestor.parent();
    }
    qualified
}

/// First descendant (inclusive) with the given kind, depth-first.
fn find_descendant<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    if node.kind() == kind {
        return Some(node);
    }
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find_map(|child| find_descendant(child, kind))
}

fn text<'a>(node: Node, source: &'a [u8]) -> &'a str {
    std::str::from_utf8(&source[node.byte_range()]).unwrap_or("")
}

/// Signature text for a definition: everything before the body `{`, capped —
/// multi-line heads are the norm in UE code.
fn headline(definition: Node, source: &[u8]) -> String {
    let end = definition
        .child_by_field_name("body")
        .map(|body| body.start_byte())
        .unwrap_or(definition.end_byte());
    let head = std::str::from_utf8(&source[definition.start_byte()..end]).unwrap_or("");
    head.trim().chars().take(300).collect()
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

    #[test]
    fn extracts_cpp_class_without_compiler() {
        let (symbols, _) = CppScanner.scan("class UWeaponInstance {};", "Source/Weapon.h");
        assert_eq!(symbols[0].name, "UWeaponInstance");
        assert_eq!(symbols[0].kind, "class");
        assert_eq!(symbols[0].role, "definition");
    }

    #[test]
    fn struct_is_a_class_specifier_with_struct_kind() {
        let (symbols, _) = CppScanner.scan("struct FHeatState { float Value; };", "x.h");
        assert_eq!(symbols[0].name, "FHeatState");
        assert_eq!(symbols[0].kind, "struct");
    }

    #[test]
    fn forward_declaration_is_not_recorded_as_definition() {
        let (symbols, _) = CppScanner.scan("class ULyraHealthComponent;", "Fwd.h");
        assert!(symbols.is_empty());
        let (structs, _) = CppScanner.scan("struct FGameplayTag;", "Fwd.h");
        assert!(structs.is_empty());
    }

    #[test]
    fn skips_export_macro_and_captures_real_class_name() {
        let (symbols, _) = CppScanner.scan(
            "class LYRAGAME_API ULyraHealthComponent : public UGameFrameworkComponent {}",
            "LyraHealthComponent.h",
        );
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "ULyraHealthComponent");
    }

    #[test]
    fn single_line_definition_with_trailing_semicolon_is_kept() {
        let (symbols, _) = CppScanner.scan("class FRangedWeaponFiringInput {};", "x.h");
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "FRangedWeaponFiringInput");
    }

    #[test]
    fn extracts_call_edges_inside_function_body() {
        let src = "\
void UWeapon::Fire()
{
    WeaponTrace();
    DoSingleBulletTrace();
    if (bReady)
    {
        OnDamageTarget();
    }
}
";
        let (_, edges) = CppScanner.scan(src, "Source/Weapon.cpp");
        let calls: Vec<_> = edges
            .iter()
            .filter(|e| e.relation == "call")
            .map(|e| (e.source_symbol.as_str(), e.target_symbol.as_str()))
            .collect();
        assert!(calls.contains(&("Fire", "WeaponTrace")));
        assert!(calls.contains(&("Fire", "DoSingleBulletTrace")));
        assert!(calls.contains(&("Fire", "OnDamageTarget")));
        assert!(!calls.iter().any(|(_, callee)| *callee == "if"));
    }

    #[test]
    fn member_and_scoped_calls_resolve_to_leaf_name() {
        let src = "\
void UWeapon::Fire()
{
    GetWorld()->GetTimerManager().SetTimer();
    UKismetMathLibrary::Sin(Angle);
}
";
        let (_, edges) = CppScanner.scan(src, "x.cpp");
        let callees: Vec<_> = edges.iter().map(|e| e.target_symbol.as_str()).collect();
        assert!(callees.contains(&"SetTimer"));
        assert!(callees.contains(&"GetTimerManager"));
        assert!(callees.contains(&"Sin"));
    }

    #[test]
    fn ue_class_with_generated_body_only_is_extracted() {
        let src = "\
UCLASS(BlueprintType, EditInlineNew, meta = (DisplayName = \"Stagger\"))
class SKILLRUNTIME_API USkillHitReaction_Stagger : public USkillHitReaction
{
\tGENERATED_BODY()
};
";
        let (symbols, _) = CppScanner.scan(src, "SkillHitReaction.h");
        let classes: Vec<_> = symbols
            .iter()
            .filter(|s| s.kind == "class")
            .map(|s| s.name.as_str())
            .collect();
        assert!(
            classes.contains(&"USkillHitReaction_Stagger"),
            "expected Stagger class, got {classes:?}"
        );
    }

    #[test]
    fn real_skill_fragments_header_extracts_base_class() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/SkillFragment.h");
        let content = std::fs::read_to_string(path).expect("read SkillFragment.h");
        let (symbols, _) = CppScanner.scan(&content, path);
        let names: Vec<_> = symbols
            .iter()
            .filter(|s| s.kind == "class")
            .map(|s| (s.name.as_str(), s.line))
            .collect();
        assert!(
            names.contains(&("USkillFragment", 16)),
            "base class missing, got {names:?}"
        );
    }

    #[test]
    fn diagnostic_real_skill_fragments_parse_tree() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/SkillFragment.h");
        let content = std::fs::read_to_string(path).expect("read SkillFragment.h");
        let terminated = terminate_ue_macros(&content);
        let cleaned = strip_api_macros(&terminated);
        let bytes = cleaned.as_bytes();
        PARSER.with(|cell| {
            let tree = cell.borrow_mut().parse(&*cleaned, None).expect("parse");
            eprintln!("root has_error = {}", tree.root_node().has_error());
            let mut stack = vec![tree.root_node()];
            while let Some(node) = stack.pop() {
                let kind = node.kind();
                let row = node.start_position().row + 1;
                if (10..=19).contains(&row) && kind != "identifier" {
                    eprintln!(
                        "L{row} {kind} [{:?}]",
                        node.utf8_text(bytes)
                            .unwrap_or("")
                            .chars()
                            .take(70)
                            .collect::<String>()
                    );
                }
                for i in 0..node.child_count() {
                    stack.push(node.child(i as u32).unwrap());
                }
            }
        });
    }

    #[test]
    fn function_role_distinguishes_definition_from_prototype() {
        let src = "class UWeaponInstance {};
void FireWeapon();
void UWeapon::Fire()
{
    WeaponTrace();
}
int UWeapon::Heat()
{
    return 0;
}
";
        let (symbols, _) = CppScanner.scan(src, "Source/Weapon.h");
        let role_of = |name: &str| {
            symbols
                .iter()
                .find(|s| s.name == name)
                .map(|s| s.role.as_str())
        };
        assert_eq!(role_of("UWeaponInstance"), Some("definition"));
        assert_eq!(role_of("FireWeapon"), Some("declaration"));
        assert_eq!(role_of("Fire"), Some("definition"));
        assert_eq!(role_of("Heat"), Some("definition"));
        let fire = symbols.iter().find(|s| s.name == "Fire").unwrap();
        assert_eq!(fire.qualified_name, "UWeapon::Fire");
    }

    #[test]
    fn multi_line_signature_is_a_definition() {
        // Regex edition's known blind spot: the parameter list spanning lines
        // made the head invisible and downgraded the symbol to a declaration.
        let src = "\
void UWeapon::Fire(
    const FVector& Direction,
    float Spread)
{
    WeaponTrace();
}
";
        let (symbols, edges) = CppScanner.scan(src, "x.cpp");
        let fire = symbols.iter().find(|s| s.name == "Fire").unwrap();
        assert_eq!(fire.role, "definition");
        assert!(edges.iter().any(|e| e.target_symbol == "WeaponTrace"));
    }

    #[test]
    fn braces_in_strings_and_comments_do_not_confuse_scope() {
        // Regex edition lost caller attribution when literals carried braces.
        let src = "\
void Outer()
{
    const char* s = \"}\";
    // }}}
    Inner();
}
";
        let (_, edges) = CppScanner.scan(src, "x.cpp");
        assert!(
            edges
                .iter()
                .any(|e| e.source_symbol == "Outer" && e.target_symbol == "Inner")
        );
    }

    #[test]
    fn ue_macros_do_not_create_bogus_symbols() {
        let src = "\
UCLASS()
class LYRAGAME_API UWeaponInstance : public UObject
{
    GENERATED_BODY()
    UFUNCTION()
    void Fire();
};
";
        let (symbols, edges) = CppScanner.scan(src, "Weapon.h");
        assert!(symbols.iter().any(|s| s.name == "UWeaponInstance"));
        assert!(symbols.iter().any(|s| s.name == "Fire"));
        assert!(!symbols.iter().any(|s| s.name == "GENERATED_BODY"));
        assert!(!edges.iter().any(|e| e.target_symbol == "GENERATED_BODY"));
    }

    #[test]
    fn prototype_does_not_open_a_scope() {
        let src = "\
void Declared();
void Real()
{
    Helper();
}
";
        let (_, edges) = CppScanner.scan(src, "x.cpp");
        let calls: Vec<_> = edges
            .iter()
            .filter(|e| e.relation == "call")
            .map(|e| (e.source_symbol.clone(), e.target_symbol.clone()))
            .collect();
        assert_eq!(calls, vec![("Real".to_string(), "Helper".to_string())]);
    }




    #[test]
    fn extracts_inheritance_edges() {
        let src = "class UWeapon : public UObject, public IAbilitySourceInterface {};";
        let (_, edges) = CppScanner.scan(src, "x.h");
        let bases: Vec<_> = edges
            .iter()
            .filter(|e| e.relation == "inherits")
            .map(|e| e.target_symbol.as_str())
            .collect();
        assert!(bases.contains(&"UObject"));
        assert!(bases.contains(&"IAbilitySourceInterface"));
        assert!(edges.iter().all(|e| e.source_symbol == "UWeapon"));
    }

    #[test]
    fn extracts_member_variables_and_their_types() {
        let src = "\nUCLASS()\nclass UWeaponInstance : public UObject\n{\n    GENERATED_BODY()\n    UPROPERTY(EditAnywhere)\n    ULyraWeaponInstance* Weapon;\n    UPROPERTY()\n    TArray<FHeatState> HeatHistory;\n    float CurrentHeat;\n};\n";
        let (symbols, edges) = CppScanner.scan(src, "Weapon.h");
        let field = symbols.iter().find(|s| s.name == "Weapon").unwrap();
        assert_eq!(field.kind, "field");
        assert_eq!(field.qualified_name, "UWeaponInstance::Weapon");
        assert!(symbols.iter().any(|s| s.name == "HeatHistory" && s.kind == "field"));
        assert!(symbols.iter().any(|s| s.name == "CurrentHeat" && s.kind == "field"));
        // primitive_type has no uses_type edge; class/template types do
        let uses: Vec<_> = edges
            .iter()
            .filter(|e| e.relation == "uses_type")
            .map(|e| (e.source_symbol.as_str(), e.target_symbol.as_str()))
            .collect();
        assert!(uses.contains(&("Weapon", "ULyraWeaponInstance")));
        assert!(uses.contains(&("HeatHistory", "TArray")));
        assert!(uses.contains(&("HeatHistory", "FHeatState")));
        assert!(!uses.iter().any(|(_, t)| *t == "float"));
    }

    #[test]
    fn extracts_type_uses_from_function_signature() {
        let src = "\nULyraWeaponInstance* UAbility::GetWeapon(const FHeatState& State, int32 Count)\n{\n    return nullptr;\n}\n";
        let (_, edges) = CppScanner.scan(src, "x.cpp");
        let uses: Vec<_> = edges
            .iter()
            .filter(|e| e.relation == "uses_type")
            .map(|e| e.target_symbol.as_str())
            .collect();
        assert!(uses.contains(&"ULyraWeaponInstance"));
        assert!(uses.contains(&"FHeatState"));
        assert!(!uses.contains(&"int32"));
        assert!(edges.iter().all(|e| e.source_symbol == "GetWeapon"));
    }

    #[test]
    fn extracts_field_reads_and_writes() {
        let src = "\nclass UWeaponInstance : public UObject\n{\n    float CurrentHeat;\n    float CoolRate;\n    float Spread;\n};\nvoid UWeaponInstance::Tick(float Dt)\n{\n    CurrentHeat = CurrentHeat - CoolRate * Dt;\n    this->Spread += 1.0f;\n}\n";
        let (_, edges) = CppScanner.scan(src, "x.cpp");
        let has = |rel: &str, target: &str| {
            edges
                .iter()
                .any(|e| e.relation == rel && e.target_symbol == target)
        };
        assert!(has("writes", "CurrentHeat"));
        assert!(has("writes", "Spread"));
        assert!(has("reads", "CurrentHeat"));
        assert!(has("reads", "CoolRate"));
    }

    #[test]
    fn local_shadow_is_not_a_field_reference() {
        let src = "\nclass UWeaponInstance\n{\n    float CurrentHeat;\n};\nvoid UWeaponInstance::Tick()\n{\n    float CurrentHeat = 0.0f;\n    CurrentHeat = 1.0f;\n}\n";
        let (_, edges) = CppScanner.scan(src, "x.cpp");
        assert!(
            !edges
                .iter()
                .any(|e| (e.relation == "reads" || e.relation == "writes")
                    && e.target_symbol == "CurrentHeat")
        );
    }

    #[test]
    fn parameters_are_not_field_references() {
        let src = "\nclass UWeaponInstance\n{\n    float CurrentHeat;\n};\nvoid UWeaponInstance::Apply(float CurrentHeat)\n{\n    Cool(CurrentHeat);\n}\n";
        let (_, edges) = CppScanner.scan(src, "x.cpp");
        assert!(
            !edges
                .iter()
                .any(|e| (e.relation == "reads" || e.relation == "writes")
                    && e.target_symbol == "CurrentHeat")
        );
    }


    #[test]
    fn all_caps_class_name_is_not_stripped_as_macro() {
        // The API-macro stripper must only fire when a *second* name token
        // follows; an all-caps class name is not a macro.
        let (symbols, _) = CppScanner.scan("class FJSONValue {};", "x.h");
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "FJSONValue");
        let (symbols, _) = CppScanner.scan("class LYRAGAME_API UWeapon final {};", "x.h");
        assert_eq!(symbols[0].name, "UWeapon");
    }

    #[test]
    fn lambda_calls_attribute_to_enclosing_function() {
        let src = "\
void Outer()
{
    auto L = []()
    {
        Inner();
    };
}
";
        let (_, edges) = CppScanner.scan(src, "x.cpp");
        assert!(
            edges
                .iter()
                .any(|e| e.source_symbol == "Outer" && e.target_symbol == "Inner")
        );
    }
}
